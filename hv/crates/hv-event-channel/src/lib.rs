use hv_alchemy::Type;
use hv_lua::{FromLua, ToLua, UserData, UserDataMethods};
use hv_sync::cell::{ArcCell, ArcRef, ArcRefMut};

impl<T: 'static + Send + Sync> Default for EventChannel<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct EventChannel<T: 'static> {
    channel: ArcCell<shrev::EventChannel<T>>,
}

impl<T: 'static + Send + Sync> EventChannel<T> {
    pub fn new() -> Self {
        Self {
            channel: ArcCell::new(shrev::EventChannel::new()),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            channel: ArcCell::new(shrev::EventChannel::with_capacity(capacity)),
        }
    }

    pub fn write(&mut self) -> WriteToken<T> {
        WriteToken {
            channel: self.channel.borrow_mut(),
        }
    }

    pub fn read(&self) -> ReadToken<T> {
        ReadToken {
            channel: self.channel.borrow(),
        }
    }
}

#[derive(Debug)]
pub struct WriteToken<T: 'static> {
    channel: ArcRefMut<shrev::EventChannel<T>>,
}

impl<T: 'static + Send + Sync> WriteToken<T> {
    pub fn would_write(&mut self) -> bool {
        self.channel.would_write()
    }

    pub fn register_reader(&mut self) -> ReaderId<T> {
        ReaderId {
            reader_id: self.channel.register_reader(),
        }
    }

    pub fn single_write(&mut self, event: T) {
        self.channel.single_write(event);
    }

    pub fn iter_write<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        self.channel.iter_write(iter);
    }
}

#[derive(Debug)]
pub struct ReadToken<T: 'static> {
    channel: ArcRef<shrev::EventChannel<T>>,
}

impl<T: 'static> Clone for ReadToken<T> {
    fn clone(&self) -> Self {
        Self {
            channel: ArcRef::clone(&self.channel),
        }
    }
}

impl<T: 'static + Send + Sync> ReadToken<T> {
    pub fn read(&self, reader_id: &mut ReaderId<T>) -> impl Iterator<Item = &'_ T> + '_ {
        self.channel.read(&mut reader_id.reader_id)
    }
}

pub struct ReaderId<T: 'static> {
    reader_id: shrev::ReaderId<T>,
}

impl<T: 'static + Send + Sync + for<'lua> FromLua<'lua> + for<'lua> ToLua<'lua> + Clone> UserData
    for EventChannel<T>
{
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("write", |_, this, ()| Ok(this.write()));
        methods.add_method("read", |_, this, ()| Ok(this.read()));
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M)
    where
        Self: 'static + Send,
    {
        methods.add_function("new", |_, ()| Ok(Self::new()));
        methods.add_function("with_capacity", |_, capacity| {
            Ok(Self::with_capacity(capacity))
        });
    }
}

impl<T: 'static + Send + Sync + for<'lua> FromLua<'lua> + for<'lua> ToLua<'lua> + Clone> UserData
    for WriteToken<T>
{
    #[allow(clippy::unit_arg)]
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("would_write", |_, this, ()| Ok(this.would_write()));
        methods.add_method_mut("register_reader", |_, this, ()| Ok(this.register_reader()));
        methods.add_method_mut("write", |_, this, ev: T| Ok(this.single_write(ev)));
        methods.add_method_mut("write_all", |_, this, evs: Vec<T>| Ok(this.iter_write(evs)));
    }
}

impl<T: 'static + Send + Sync + for<'lua> FromLua<'lua> + for<'lua> ToLua<'lua> + Clone> UserData
    for ReadToken<T>
{
}

impl<T: 'static + Send + Sync + for<'lua> FromLua<'lua> + for<'lua> ToLua<'lua> + Clone> UserData
    for ReaderId<T>
{
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("read", |_, this, token: ReadToken<T>| {
            Ok(token.read(this).cloned().collect::<Vec<_>>())
        });
    }
}
