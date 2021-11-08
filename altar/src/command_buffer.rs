use hv::{
    bump::{Owned, *},
    ecs::{DynamicBundle, Entity, EntityBuilder, World},
    prelude::*,
    sync::{
        cell::AtomicRef,
        elastic::{Elastic, ElasticGuard, Stretchable, Stretched},
        NoSharedAccess,
    },
};
use resources::Resources;

use spin::Mutex;
use std::{cell::UnsafeCell, mem::ManuallyDrop, sync::Arc};
use tracing::{error, trace_span, warn};

type Command<'a> = Owned<'a, dyn FnMut(&mut World, &mut Resources) -> Result<()> + Send>;

const CHUNK_SIZE: usize = 1024;

pub struct CommandBuffer {
    inner: Elastic<StretchedCommandBufferInner>,
}

static_assertions::assert_impl_all!(CommandBuffer: LuaUserData, Send, Sync);

impl CommandBuffer {
    pub fn push(
        &mut self,
        command: impl FnOnce(&mut World, &mut Resources) -> Result<()> + Send + 'static,
    ) {
        let mut inner = self.inner.borrow_mut().unwrap();
        let bump = unsafe { &*inner.bump.get() };
        let mut command = Some(command);
        let wrapped = move |world: &'_ mut World, resources: &'_ mut Resources| {
            (command.take().unwrap())(world, resources)
        };
        let owned: Command = unsafe {
            Owned::from_raw(Owned::into_raw(bump.alloc_boxed(wrapped))
                as *mut (dyn FnMut(&mut World, &mut Resources) -> Result<()> + Send))
        };

        if let Err(command) = inner.chunk.push(owned) {
            let mut new_chunk = bump.chunk(CHUNK_SIZE);
            new_chunk
                .push(command)
                .ok()
                .expect("fresh chunk should have space");
            let old_chunk = std::mem::replace(&mut inner.chunk, new_chunk);

            if !old_chunk.is_empty() {
                inner.bufs.lock().push(old_chunk);
            }
        }
    }

    pub fn spawn(&mut self, bundle: impl DynamicBundle + Send + 'static) {
        self.push(move |world, _| {
            world.spawn(bundle);
            Ok(())
        });
    }

    pub fn insert(&mut self, entity: Entity, bundle: impl DynamicBundle + Send + 'static) {
        self.push(move |world, _| {
            world.insert(entity, bundle)?;
            Ok(())
        });
    }

    pub fn despawn(&mut self, entity: Entity) {
        self.push(move |world, _| {
            world.despawn(entity)?;
            Ok(())
        });
    }
}

impl LuaUserData for CommandBuffer {
    fn on_metatable_init(table: Type<Self>) {
        table.add::<dyn Send>().add::<dyn Sync>();
    }

    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("push", |lua, this, function: LuaFunction| {
            let key = lua.create_registry_value(function)?;
            this.push(move |_, resources| {
                let mut nsa_lua = resources.get_mut::<NoSharedAccess<Lua>>().to_lua_err()?;
                let lua = nsa_lua.get_mut();
                let f: LuaFunction = lua.registry_value(&key)?;
                let _: () = f.call(())?;
                Ok(())
            });
            Ok(())
        });

        methods.add_method_mut("spawn", |_, this, mut bundle: EntityBuilder| {
            this.push(move |world, _| {
                world.spawn(bundle.build());
                Ok(())
            });

            Ok(())
        });
    }
}

#[repr(C, align(8))]
struct StretchedCommandBufferInner([u8; std::mem::size_of::<CommandBufferInner>()]);

static_assertions::assert_eq_align!(CommandBufferInner, StretchedCommandBufferInner);

struct CommandBufferInner<'a> {
    bump: UnsafeCell<PooledBump<'a>>,
    chunk: Chunk<'a, Command<'a>>,
    bufs: Arc<Mutex<Vec<Chunk<'a, Command<'a>>>>>,
}

unsafe impl<'a> Send for CommandBufferInner<'a> {}
unsafe impl<'a> Sync for CommandBufferInner<'a> {}

impl<'a> Drop for CommandBufferInner<'a> {
    fn drop(&mut self) {
        // On drop, flush any remaining commands.
        let old_chunk = std::mem::replace(&mut self.chunk, Chunk::new(&mut []));

        if !old_chunk.is_empty() {
            self.bufs.lock().push(old_chunk);
        }
    }
}

impl<'a> Stretchable<'a> for CommandBufferInner<'a> {
    type Stretched = StretchedCommandBufferInner;
}

unsafe impl Stretched for StretchedCommandBufferInner {
    type Parameterized<'a> = CommandBufferInner<'a>;

    hv::sync::impl_stretched_methods!(std);
}

pub struct CommandPool {
    stampede: BumpPool,
    raw_chunk_bufs: Mutex<Vec<(*mut (), usize, usize)>>,
    raw_elastic_bufs: Mutex<Vec<(*mut (), usize, usize)>>,
    raw_guard_bufs: Mutex<Vec<(*mut (), usize, usize)>>,
}

pub struct CommandPoolScope<'a> {
    command_pool: &'a CommandPool,
    buf: ManuallyDrop<Arc<Mutex<Vec<Chunk<'a, Command<'a>>>>>>,
    guards: ManuallyDrop<Mutex<Vec<ElasticGuard<'a, CommandBufferInner<'a>>>>>,
    elastics: ManuallyDrop<Mutex<Vec<Elastic<StretchedCommandBufferInner>>>>,
}

impl CommandPool {
    pub fn scope(&'_ self) -> CommandPoolScope<'_> {
        CommandPoolScope {
            command_pool: self,
            buf: ManuallyDrop::new(Arc::new(Mutex::new(
                self.raw_chunk_bufs
                    .lock()
                    .pop()
                    .map(|(ptr, len, cap)| unsafe { Vec::from_raw_parts(ptr.cast(), len, cap) })
                    .unwrap_or_default(),
            ))),
            guards: ManuallyDrop::new(Mutex::new(
                self.raw_guard_bufs
                    .lock()
                    .pop()
                    .map(|(ptr, len, cap)| unsafe { Vec::from_raw_parts(ptr.cast(), len, cap) })
                    .unwrap_or_default(),
            )),
            elastics: ManuallyDrop::new(Mutex::new(
                self.raw_elastic_bufs
                    .lock()
                    .pop()
                    .map(|(ptr, len, cap)| unsafe { Vec::from_raw_parts(ptr.cast(), len, cap) })
                    .unwrap_or_default(),
            )),
        }
    }
}

impl<'a> CommandPoolScope<'a> {
    pub fn get(&self) -> CommandBuffer {
        let bump = UnsafeCell::new(self.command_pool.stampede.get());
        let chunk = Chunk::new(&mut []);
        let elastic = self.elastics.lock().pop().unwrap_or_default();
        let inner = CommandBufferInner {
            bump,
            chunk,
            bufs: (*self.buf).clone(),
        };
        let guard = elastic.loan(inner);
        self.guards.lock().push(guard);

        CommandBuffer { inner: elastic }
    }

    pub fn flush(mut self, world: &mut World, resources: &mut Resources) {
        // Empty all elastics by destroying their guards, also dumping any remaining commands into
        // the queue.
        self.guards.get_mut().clear();

        let bufs = Arc::get_mut(&mut self.buf)
            .expect(
                "Strong reference to command chunks buffer still exists after clearing elastics?",
            )
            .get_mut();

        for mut command in bufs.drain(..).flatten() {
            let res = command(world, resources);

            if let Err(err) = res {
                error!(error = ?err, "error calling buffered command: {:#}", err);
            }
        }
    }
}

impl<'a> Drop for CommandPoolScope<'a> {
    fn drop(&mut self) {
        let _span = trace_span!("CommandPoolScope::drop");

        // Empty all the remaining elastics, by destroying their guards.
        self.guards.get_mut().clear();
        // Destroy any remaining commands without running them; if there are remaining commands, log
        // a warning.
        let bufs = Arc::get_mut(&mut self.buf).expect("CommandPoolScope should have only strong reference after all elastic guards dropped!!").get_mut();
        if !bufs.is_empty() {
            warn!(
                len = bufs.len(),
                "command pool scope was dropped with {} unflushed commands",
                bufs.len()
            );

            bufs.clear();
        }

        let bufs = Arc::try_unwrap(unsafe { ManuallyDrop::take(&mut self.buf) })
            .ok()
            .unwrap()
            .into_inner();
        let guards = unsafe { ManuallyDrop::take(&mut self.guards) }.into_inner();
        let elastics = unsafe { ManuallyDrop::take(&mut self.elastics) }.into_inner();

        {
            let (ptr, len, cap) = Vec::into_raw_parts(bufs);
            self.command_pool
                .raw_chunk_bufs
                .lock()
                .push((ptr.cast(), len, cap));
        }

        {
            let (ptr, len, cap) = Vec::into_raw_parts(guards);
            self.command_pool
                .raw_guard_bufs
                .lock()
                .push((ptr.cast(), len, cap));
        }

        {
            let (ptr, len, cap) = Vec::into_raw_parts(elastics);
            self.command_pool
                .raw_elastic_bufs
                .lock()
                .push((ptr.cast(), len, cap));
        }
    }
}

#[repr(C, align(8))]
pub struct StretchedCommandPoolScope([u8; std::mem::size_of::<CommandPoolScope>()]);

static_assertions::assert_eq_align!(StretchedCommandPoolScope, CommandPoolScope);

unsafe impl Stretched for StretchedCommandPoolScope {
    type Parameterized<'a> = CommandPoolScope<'a>;

    hv::sync::impl_stretched_methods!(std);
}

impl<'a> Stretchable<'a> for CommandPoolScope<'a> {
    type Stretched = StretchedCommandPoolScope;
}

/// A lifetime-less resource type intended to be placed into a [`Resources`] struct, which can be
/// loaned a [`CommandPoolScope`] to enable it to be properly borrowed.
#[derive(Clone, Default)]
pub struct CommandPoolResource {
    inner: Elastic<StretchedCommandPoolScope>,
}

static_assertions::assert_impl_all!(CommandPoolResource: LuaUserData, Send, Sync);

pub struct CommandPoolGuard<'g>(ElasticGuard<'g, CommandPoolScope<'g>>);

impl CommandPoolResource {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn loan<'g>(&self, scope: CommandPoolScope<'g>) -> CommandPoolGuard<'g> {
        CommandPoolGuard(self.inner.loan(scope))
    }

    pub fn borrow(&self) -> AtomicRef<CommandPoolScope> {
        self.inner
            .borrow()
            .expect("command pool resource should never be mutably borrowed")
    }

    pub fn get_buffer(&self) -> CommandBuffer {
        self.borrow().get()
    }
}

impl LuaUserData for CommandPoolResource {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().add::<dyn Send>().add::<dyn Sync>();
    }

    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get_buffer", |_, this, ()| Ok(this.get_buffer()));
    }
}
