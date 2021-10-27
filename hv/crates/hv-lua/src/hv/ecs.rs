use std::any::TypeId;

use hv_alchemy::{AlchemicalAny, TypedAlchemyTable};
use hv_sync::elastic::Elastic;

use crate::{
    AnyUserData, Error, ExternalResult, FromLua, Function, LightUserData, Lua, MultiValue, Result,
    Table, ToLua, ToLuaMulti, UserData, UserDataMethods, Value,
};

impl UserData for hecs::ColumnBatchType {
    fn on_metatable_init(table: TypedAlchemyTable<Self>) {
        table.mark_clone().add::<dyn Send>().add::<dyn Sync>();
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

    fn add_type_methods<'lua, M: UserDataMethods<'lua, TypedAlchemyTable<Self>>>(methods: &mut M)
    where
        Self: 'static,
    {
        methods.add_function("new", |_, ()| Ok(Self::new()));
    }
}

impl UserData for hecs::ColumnBatchBuilder {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut(
            "writer",
            |lua, this, (ty, scope): (AnyUserData, Function)| {
                let (guard, writer) = ty
                    .dyn_borrow::<dyn ComponentType>()?
                    .column_batch_builder_writer(lua, this)?;
                let res = scope.call::<_, MultiValue>(writer);
                drop(guard);
                res
            },
        );
    }
}

impl UserData for hecs::ColumnBatch {}

impl<T: 'static + UserData> UserData for Elastic<hecs::BatchWriter<'static, T>> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("push", |_, this, ud: AnyUserData| {
            this.borrow_mut()
                .ok_or_else(|| Error::external("BatchWriter already destructed!"))?
                .push(*ud.clone_or_take::<T>()?)
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

struct RawSingleBundle {
    data: *mut u8,
    owning: bool,
    info: hecs::TypeInfo,
}

impl<'lua> FromLua<'lua> for RawSingleBundle {
    fn from_lua(lua_value: Value<'lua>, lua: &'lua Lua) -> Result<Self> {
        let ud = AnyUserData::from_lua(lua_value, lua)?;
        let borrowed = ud.dyn_borrow::<dyn AlchemicalAny>()?;
        let alchemy_table = (*borrowed).alchemy_table();

        if !(alchemy_table.is::<dyn Send>() && alchemy_table.is::<dyn Sync>()) {
            return Err(Error::external(format!(
                "userdata type `{}` is not registered as Send + Sync!",
                alchemy_table.type_name
            )));
        }

        let owning = !alchemy_table.is_copy();
        let data;

        if owning {
            unsafe {
                data = std::alloc::alloc(alchemy_table.layout);
                drop(borrowed);
                let mut borrowed_mut = ud.dyn_borrow_mut::<dyn AlchemicalAny>()?;
                if hv_alchemy::clone_or_move(&mut *borrowed_mut, data as *mut _) {
                    std::alloc::dealloc(
                        Box::into_raw(ud.dyn_take()?) as *mut u8,
                        alchemy_table.layout,
                    );
                }
            }
        } else {
            data = (&*borrowed) as *const _ as *mut dyn AlchemicalAny as *mut u8;
        }

        let info = hecs::TypeInfo {
            id: alchemy_table.id,
            layout: alchemy_table.layout,
            drop: alchemy_table.drop,
            #[cfg(debug_assertions)]
            type_name: alchemy_table.type_name,
        };

        Ok(RawSingleBundle { data, owning, info })
    }
}

unsafe impl hecs::DynamicBundle for RawSingleBundle {
    fn key(&self) -> Option<TypeId> {
        Some(self.info.id)
    }

    fn with_ids<T>(&self, f: impl FnOnce(&[TypeId]) -> T) -> T {
        f(&[self.info.id])
    }

    fn type_info(&self) -> Vec<hecs::TypeInfo> {
        vec![self.info]
    }

    unsafe fn put(self, mut f: impl FnMut(*mut u8, hecs::TypeInfo)) {
        f(self.data as *mut u8, self.info);
        if self.owning {
            // Drop the box to deallocate the memory but not drop the actual component there, since it
            // has now been moved.
            std::alloc::dealloc(self.data, self.info.layout);
        }
    }
}

impl<'lua> ToLua<'lua> for hecs::Entity {
    #[inline]
    fn to_lua(self, _lua: &'lua Lua) -> Result<Value<'lua>> {
        Ok(Value::LightUserData(LightUserData(
            self.to_bits().get() as *mut _
        )))
    }
}

impl<'lua> FromLua<'lua> for hecs::Entity {
    #[inline]
    fn from_lua(lua_value: Value<'lua>, lua: &'lua Lua) -> Result<Self> {
        LightUserData::from_lua(lua_value, lua).and_then(|lud| {
            hecs::Entity::from_bits(lud.0 as u64)
                .ok_or_else(|| Error::external("invalid entity ID (zero)"))
        })
    }
}

pub trait ComponentType: Send + Sync {
    fn type_id(&self) -> TypeId;

    fn read(&self) -> hecs::DynamicQuery;
    fn write(&self) -> hecs::DynamicQuery;

    fn column_batch_type_add(&self, column_batch_type: &mut hecs::ColumnBatchType);
    fn column_batch_builder_writer<'lua, 'a>(
        &self,
        lua: &'lua Lua,
        column_batch: &'a mut hecs::ColumnBatchBuilder,
    ) -> Result<(Box<dyn Send + 'a>, AnyUserData<'lua>)>;

    fn dynamic_item_take<'lua>(
        &self,
        lua: &'lua Lua,
        dynamic_item: &mut hecs::DynamicItem,
    ) -> Result<Option<AnyUserData<'lua>>>;
}

impl<T: hecs::Component + UserData> ComponentType for TypedAlchemyTable<T> {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn read(&self) -> hecs::DynamicQuery {
        hecs::DynamicQuery::lift::<&T>()
    }

    fn write(&self) -> hecs::DynamicQuery {
        hecs::DynamicQuery::lift::<&mut T>()
    }

    fn column_batch_type_add(&self, column_batch_type: &mut hecs::ColumnBatchType) {
        column_batch_type.add::<T>();
    }

    fn column_batch_builder_writer<'lua, 'a>(
        &self,
        lua: &'lua Lua,
        column_batch_builder: &'a mut hecs::ColumnBatchBuilder,
    ) -> Result<(Box<dyn Send + 'a>, AnyUserData<'lua>)> {
        let elastic = <Elastic<hecs::BatchWriter<'static, T>>>::new();
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
        dynamic_item: &mut hecs::DynamicItem,
    ) -> Result<Option<AnyUserData<'lua>>> {
        dynamic_item
            .take::<T>()
            .map(|c| lua.create_userdata(c))
            .transpose()
    }
}

impl UserData for hecs::DynamicQuery {
    fn add_type_methods<'lua, M: UserDataMethods<'lua, TypedAlchemyTable<Self>>>(methods: &mut M) {
        methods.add_function("new", move |_, table: Table| {
            let mut free_elements = Vec::new();
            for try_element in table.sequence_values::<hecs::DynamicQuery>() {
                free_elements.push(try_element?);
            }

            let q = hecs::DynamicQuery::new(free_elements);

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

impl UserData for hecs::DynamicItem {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("take", move |lua, this, ty: AnyUserData| {
            ty.dyn_borrow::<dyn ComponentType>()?
                .dynamic_item_take(lua, this)
        });
    }
}

impl UserData for hecs::World {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("contains", |_lua, this, entity: hecs::Entity| {
            Ok(this.contains(entity))
        });

        methods.add_method("len", |_lua, this, ()| Ok(this.len()));

        let mut builder = hecs::EntityBuilder::new();
        methods.add_method_mut("spawn", move |lua, this, components: Table| {
            for component in components.sequence_values() {
                builder.add_bundle(RawSingleBundle::from_lua(component?, lua)?);
            }

            Ok(this.spawn(builder.build()))
        });

        methods.add_method(
            "query",
            |lua, this, (query, for_each): (hecs::DynamicQuery, Function<'lua>)| {
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
            |_lua, this, (query, entity, for_entity): (hecs::DynamicQuery, hecs::Entity,Function<'lua>)| {
                let mut dynamic_query_one = this.dynamic_query_one(&query, entity).to_lua_err()?;
                let out = for_entity.call::<_, MultiValue>(dynamic_query_one.get());
                drop(dynamic_query_one);
                out
            },
        );
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, TypedAlchemyTable<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, ()| Ok(hecs::World::new()));
    }
}
