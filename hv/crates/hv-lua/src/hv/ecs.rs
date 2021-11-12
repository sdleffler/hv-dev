use std::{any::TypeId, cell::Ref, mem::MaybeUninit};

use hv_alchemy::{AlchemicalAny, Type};
use hv_ecs as ecs;
use hv_sync::elastic::{external::ecs::StretchedBatchWriter, Elastic};

use crate::{
    userdata::{UserDataFieldsProxy, UserDataMethodsProxy},
    AnyUserData, Error, ExternalResult, FromLua, Function, LightUserData, Lua, MultiValue, Result,
    Table, ToLua, ToLuaMulti, UserData, UserDataFields, UserDataMethods, Value,
};

impl<'lua> FromLua<'lua> for ecs::EntityBuilder {
    fn from_lua(lua_value: Value<'lua>, _lua: &'lua Lua) -> Result<Self> {
        let mut builder = ecs::EntityBuilder::new();
        match lua_value {
            Value::Table(table) => {
                builder.clear();
                for component in table.sequence_values::<AnyUserData>() {
                    let component = component?;
                    if let Ok(bundle) = component
                        .clone()
                        .dyn_clone_or_take::<dyn DynamicBundleProxy>()
                    {
                        builder.add_bundle(bundle);
                    } else if let Ok(single) = LuaSingleBundle::from_lua_userdata(&component) {
                        builder.add_bundle(single);
                    } else {
                        return Err(Error::external(
                            "expected a table of bundles and components",
                        ));
                    }
                }
            }
            Value::UserData(ud) => {
                builder.add_bundle(ud.dyn_clone_or_take::<dyn DynamicBundleProxy>()?);
            }
            _ => {
                return Err(Error::external(
                    "expected either a bundle or a table of bundles and components",
                ))
            }
        }

        Ok(builder)
    }
}

impl UserData for ecs::ColumnBatchType {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().add::<dyn Send>().add::<dyn Sync>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("add", |_, this, ty: AnyUserData| {
            ty.dyn_borrow::<dyn ComponentType>()?
                .column_batch_type_add(this);
            Ok(())
        });

        methods.add_function("into_batch", |_, (this, size): (AnyUserData, u32)| {
            Ok(this.take::<Self>()?.into_batch(size))
        });
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M)
    where
        Self: 'static,
    {
        methods.add_function("new", |_, ()| Ok(Self::new()));
    }
}

impl UserData for ecs::ColumnBatchBuilder {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut(
            "writer",
            |lua, this, (ty, scope): (AnyUserData, Function)| {
                // safety: guard MUST be dropped before end of scope
                let (guard, writer) = unsafe {
                    ty.dyn_borrow::<dyn ComponentType>()?
                        .column_batch_builder_writer(lua, this)?
                };
                let res = scope.call::<_, MultiValue>(writer);
                drop(guard);
                res
            },
        );
    }
}

impl UserData for ecs::ColumnBatch {}

impl<T: 'static + UserData> UserData for Elastic<StretchedBatchWriter<T>> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("push", |_, this, ud: AnyUserData| {
            this.borrow_mut()
                .ok_or_else(|| Error::external("BatchWriter already destructed!"))?
                .push(ud.clone_or_take::<T>()?)
                .ok()
                .ok_or_else(|| Error::external("BatchWriter is full!"))?;
            Ok(())
        });

        methods.add_method("fill", |_, this, ()| {
            Ok(this
                .borrow()
                .ok_or_else(|| Error::external("BatchWriter already destructed!"))?
                .fill())
        });
    }
}

#[allow(clippy::missing_safety_doc)]
pub(crate) unsafe trait DynamicBundleProxy {
    fn key(&self) -> Option<TypeId>;
    unsafe fn with_ids(&self, f: &mut dyn FnMut(&[TypeId]));
    fn type_info(&self) -> Vec<ecs::TypeInfo>;
    unsafe fn put(self: Box<Self>, f: &mut dyn FnMut(*mut u8, ecs::TypeInfo));
}

static_assertions::assert_obj_safe!(DynamicBundleProxy);

unsafe impl<T: ecs::DynamicBundle> DynamicBundleProxy for T {
    fn key(&self) -> Option<TypeId> {
        ecs::DynamicBundle::key(self)
    }

    unsafe fn with_ids(&self, f: &mut dyn FnMut(&[TypeId])) {
        ecs::DynamicBundle::with_ids(self, f)
    }

    fn type_info(&self) -> Vec<ecs::TypeInfo> {
        ecs::DynamicBundle::type_info(self)
    }

    unsafe fn put(self: Box<Self>, f: &mut dyn FnMut(*mut u8, ecs::TypeInfo)) {
        ecs::DynamicBundle::put(*self, f)
    }
}

unsafe impl ecs::DynamicBundle for Box<dyn DynamicBundleProxy> {
    fn key(&self) -> Option<TypeId> {
        <dyn DynamicBundleProxy>::key(self)
    }

    fn with_ids<T>(&self, f: impl FnOnce(&[TypeId]) -> T) -> T {
        let mut uninit = MaybeUninit::zeroed();
        let t = unsafe {
            <dyn DynamicBundleProxy>::with_ids(self, &mut |type_ids| {
                uninit.write(core::ptr::read(&f)(type_ids));
            });
            uninit.assume_init()
        };
        core::mem::forget(f);
        t
    }

    fn type_info(&self) -> Vec<ecs::TypeInfo> {
        <dyn DynamicBundleProxy>::type_info(self)
    }

    unsafe fn put(self, mut f: impl FnMut(*mut u8, ecs::TypeInfo)) {
        <dyn DynamicBundleProxy>::put(self, &mut f);
    }
}

struct LuaSingleBundle<'a> {
    data: *mut u8,
    // `None` if data is a piece of allocated heap memory that needs to be freed after use
    // if `Some`, then data is just a pointer to a piece of memory we don't own
    borrow: Option<Ref<'a, dyn AlchemicalAny>>,
    // drop flag: if this is true, no need to run the destructor for the data
    moved: bool,
    info: ecs::TypeInfo,
}

impl<'a> LuaSingleBundle<'a> {
    fn from_lua_userdata<'lua>(ud: &'a AnyUserData<'lua>) -> Result<Self> {
        let borrowed = ud.dyn_borrow::<dyn AlchemicalAny>()?;
        let type_table = (*borrowed).type_table();

        if !(type_table.is::<dyn Send>() && type_table.is::<dyn Sync>()) {
            return Err(Error::external(format!(
                "userdata type `{}` is not registered as Send + Sync!",
                type_table.type_name
            )));
        }

        let owning = !type_table.is_copy();
        let data;
        let borrow;

        if owning {
            unsafe {
                data = std::alloc::alloc(type_table.layout);
                drop(borrowed);
                let mut borrowed_mut = ud.dyn_borrow_mut::<dyn AlchemicalAny>()?;
                if hv_alchemy::clone_or_move(&mut *borrowed_mut, data as *mut _) {
                    std::alloc::dealloc(
                        Box::into_raw(ud.dyn_take::<dyn AlchemicalAny>().unwrap()) as *mut u8,
                        type_table.layout,
                    );
                }
                borrow = None;
            }
        } else {
            data = (&*borrowed) as *const _ as *mut dyn AlchemicalAny as *mut u8;
            borrow = Some(borrowed);
        }

        let info = ecs::TypeInfo {
            id: type_table.id,
            layout: type_table.layout,
            drop: type_table.drop,
            #[cfg(debug_assertions)]
            type_name: type_table.type_name,
        };

        Ok(LuaSingleBundle {
            data,
            borrow,
            moved: false,
            info,
        })
    }
}

unsafe impl ecs::DynamicBundle for LuaSingleBundle<'_> {
    fn key(&self) -> Option<TypeId> {
        Some(self.info.id)
    }

    fn with_ids<T>(&self, f: impl FnOnce(&[TypeId]) -> T) -> T {
        f(&[self.info.id])
    }

    fn type_info(&self) -> Vec<ecs::TypeInfo> {
        vec![self.info]
    }

    unsafe fn put(mut self, mut f: impl FnMut(*mut u8, ecs::TypeInfo)) {
        f(self.data as *mut u8, self.info);
        self.moved = true;
        drop(self);
    }
}

impl Drop for LuaSingleBundle<'_> {
    fn drop(&mut self) {
        // if we didn't borrow this data, we have to deallocate the associated heap allocation
        if self.borrow.is_none() {
            // if this bundle wasn't actually used/emptied, we have to deallocate the object inside,
            // too.
            if !self.moved {
                unsafe { (self.info.drop)(self.data) };
            }

            // Drop the box to deallocate the memory but not drop the actual component there, since
            // it has now been moved.
            unsafe { std::alloc::dealloc(self.data, self.info.layout) };
        }
    }
}

impl<'lua> ToLua<'lua> for ecs::Entity {
    #[inline]
    fn to_lua(self, _lua: &'lua Lua) -> Result<Value<'lua>> {
        Ok(Value::LightUserData(LightUserData(
            self.to_bits().get() as *mut _
        )))
    }
}

impl<'lua> FromLua<'lua> for ecs::Entity {
    #[inline]
    fn from_lua(lua_value: Value<'lua>, lua: &'lua Lua) -> Result<Self> {
        LightUserData::from_lua(lua_value, lua).and_then(|lud| {
            ecs::Entity::from_bits(lud.0 as u64)
                .ok_or_else(|| Error::external("invalid entity ID (zero)"))
        })
    }
}

pub trait ComponentType: Send + Sync {
    fn type_id(&self) -> TypeId;

    fn read(&self) -> ecs::DynamicQuery;
    fn write(&self) -> ecs::DynamicQuery;

    fn column_batch_type_add(&self, column_batch_type: &mut ecs::ColumnBatchType);

    /// # Safety
    ///
    /// The `Box` returned from this function contains an `ElasticGuard`. For an invocation to be
    /// safe, the guard MUST be dropped by the end of its lifetime.
    unsafe fn column_batch_builder_writer<'lua, 'a>(
        &self,
        lua: &'lua Lua,
        column_batch: &'a mut ecs::ColumnBatchBuilder,
    ) -> Result<(Box<dyn Send + 'a>, AnyUserData<'lua>)>;

    fn dynamic_item_take<'lua>(
        &self,
        lua: &'lua Lua,
        dynamic_item: &mut ecs::DynamicItem,
    ) -> Result<Option<AnyUserData<'lua>>>;
}

impl<T: ecs::Component + UserData> ComponentType for Type<T> {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn read(&self) -> ecs::DynamicQuery {
        ecs::DynamicQuery::lift::<&T>()
    }

    fn write(&self) -> ecs::DynamicQuery {
        ecs::DynamicQuery::lift::<&mut T>()
    }

    fn column_batch_type_add(&self, column_batch_type: &mut ecs::ColumnBatchType) {
        column_batch_type.add::<T>();
    }

    unsafe fn column_batch_builder_writer<'lua, 'a>(
        &self,
        lua: &'lua Lua,
        column_batch_builder: &'a mut ecs::ColumnBatchBuilder,
    ) -> Result<(Box<dyn Send + 'a>, AnyUserData<'lua>)> {
        let elastic = <Elastic<StretchedBatchWriter<T>>>::new();
        let guard = elastic.loan(
            column_batch_builder
                .writer::<T>()
                .ok_or_else(|| Error::external("not in ColumnBatch"))?,
        );
        Ok((Box::new(guard), lua.create_userdata(elastic)?))
    }

    fn dynamic_item_take<'lua>(
        &self,
        lua: &'lua Lua,
        dynamic_item: &mut ecs::DynamicItem,
    ) -> Result<Option<AnyUserData<'lua>>> {
        dynamic_item
            .take::<T>()
            .map(|c| lua.create_userdata(c))
            .transpose()
    }
}

impl UserData for ecs::DynamicQuery {
    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", move |_, table: Table| {
            let mut free_elements = Vec::new();
            for try_element in table.sequence_values::<ecs::DynamicQuery>() {
                free_elements.push(try_element?);
            }

            let q = ecs::DynamicQuery::new(free_elements);

            Ok(q)
        });

        methods.add_function("read", move |_, ty: AnyUserData| {
            Ok(ty.dyn_borrow::<dyn ComponentType>()?.read())
        });

        methods.add_function("write", move |_, ty: AnyUserData| {
            Ok(ty.dyn_borrow::<dyn ComponentType>()?.write())
        });
    }
}

impl UserData for ecs::DynamicItem {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("take", move |lua, this, ty: AnyUserData| {
            ty.dyn_borrow::<dyn ComponentType>()?
                .dynamic_item_take(lua, this)
        });
    }
}

impl<T: 'static + UserData + Send + Sync> UserData for ecs::DynamicComponent<T> {
    fn on_metatable_init(table: Type<Self>) {
        table.add::<dyn Send>().add::<dyn Sync>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        T::add_methods(&mut UserDataMethodsProxy::new(methods));
    }

    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        T::add_fields(&mut UserDataFieldsProxy::new(fields))
    }
}

impl UserData for ecs::World {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("contains", |_lua, this, entity: ecs::Entity| {
            Ok(this.contains(entity))
        });

        methods.add_method("len", |_lua, this, ()| Ok(this.len()));

        let mut builder = ecs::EntityBuilder::new();
        methods.add_method_mut("spawn", move |_, this, args: MultiValue| {
            assert_eq!(args.len(), 1, "spawn only takes one argument!");
            let arg = args.into_iter().next().unwrap();
            match arg {
                Value::Table(table) => {
                    builder.clear();
                    for component in table.sequence_values::<AnyUserData>() {
                        let component = component?;
                        if let Ok(bundle) = component
                            .clone()
                            .dyn_clone_or_take::<dyn DynamicBundleProxy>()
                        {
                            builder.add_bundle(bundle);
                        } else if let Ok(single) = LuaSingleBundle::from_lua_userdata(&component) {
                            builder.add_bundle(single);
                        } else {
                            return Err(Error::external(
                                "expected a table of bundles and components",
                            ));
                        }
                    }
                    Ok(this.spawn(builder.build()))
                }
                Value::UserData(ud) => {
                    Ok(this.spawn(ud.dyn_clone_or_take::<dyn DynamicBundleProxy>()?))
                }
                _ => Err(Error::external(
                    "expected either a bundle or a table of bundles and components",
                )),
            }
        });

        methods.add_method(
            "query",
            |lua, this, (query, for_each): (ecs::DynamicQuery, Function<'lua>)| {
                let mut dynamic_query = this.dynamic_query(&query);
                let mut dynamic_query_iter = dynamic_query.iter();
                let mut out: Option<MultiValue<'lua>> = None;
                lua.scope(|scope| {
                    let iter =
                        scope.create_function_mut(|lua, ()| match dynamic_query_iter.next() {
                            Some(pair) => pair.to_lua_multi(lua),
                            None => Value::Nil.to_lua_multi(lua),
                        })?;
                    out = Some(for_each.call(iter)?);
                    Ok(())
                })?;
                Ok(out.unwrap())
            },
        );

        methods.add_method(
            "query_one",
            |_lua, this, (query, entity, for_entity): (ecs::DynamicQuery, ecs::Entity,Function<'lua>)| {
                let mut dynamic_query_one = this.dynamic_query_one(&query, entity).to_lua_err()?;
                let out = for_entity.call::<_, MultiValue>(dynamic_query_one.get());
                drop(dynamic_query_one);
                out
            },
        );
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, ()| Ok(ecs::World::new()));
    }
}

pub fn types(lua: &Lua) -> Result<Table> {
    macro_rules! e {
        ($ty:ty as $name:ident) => {
            (stringify!($name), lua.create_userdata_type::<$ty>()?)
        };
    }

    let es = vec![
        e!(ecs::World as World),
        e!(ecs::DynamicQuery as Query),
        e!(ecs::DynamicItem as Item),
        e!(ecs::ColumnBatchType as ColumnBatchType),
    ];

    lua.create_table_from(es)
}
