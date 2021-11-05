use std::sync::Arc;

use hv_alchemy::Type;
use hv_sync::{
    cell::{ArcCell, ArcRef, ArcRefMut, AtomicRefCell},
    elastic::Elastic,
};

use crate::{
    types::{MaybeSend, MaybeSync},
    userdata::{UserDataFieldsProxy, UserDataMethodsProxy},
    UserData, UserDataFields, UserDataMethods,
};

impl<T: 'static + UserData + MaybeSend + MaybeSync> UserData for Arc<AtomicRefCell<T>> {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone();

        #[cfg(feature = "send")]
        table.add::<dyn Send>().add::<dyn Sync>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        T::add_methods(&mut UserDataMethodsProxy::new(methods));
    }

    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        T::add_fields(&mut UserDataFieldsProxy::new(fields))
    }
}

impl<T: 'static + UserData + MaybeSend + MaybeSync> UserData for ArcCell<T> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("borrow", |_lua, this, ()| Ok(this.borrow()));
        methods.add_method("borrow_mut", |_lua, this, ()| Ok(this.borrow_mut()));
        methods.add_method("try_borrow", |_lua, this, ()| Ok(this.try_borrow().ok()));
        methods.add_method("try_borrow_mut", |_lua, this, ()| {
            Ok(this.try_borrow_mut().ok())
        });
    }
}

impl<T: 'static + UserData + MaybeSend + MaybeSync> UserData for ArcRef<T> {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone();

        #[cfg(feature = "send")]
        table.add::<dyn Send>().add::<dyn Sync>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        T::add_methods(&mut UserDataMethodsProxy::new(methods));
    }

    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        T::add_fields(&mut UserDataFieldsProxy::new(fields))
    }
}

impl<T: 'static + UserData + MaybeSend + MaybeSync> UserData for ArcRefMut<T> {
    fn on_metatable_init(table: Type<Self>) {
        #[cfg(feature = "send")]
        table.add::<dyn Send>().add::<dyn Sync>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        T::add_methods(&mut UserDataMethodsProxy::new(methods));
    }

    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        T::add_fields(&mut UserDataFieldsProxy::new(fields))
    }
}

impl<T: 'static + UserData + MaybeSend + MaybeSync> UserData for Elastic<&'static mut T> {
    fn on_metatable_init(table: Type<Self>) {
        #[cfg(feature = "send")]
        table.add::<dyn Send>().add::<dyn Sync>();
    }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        T::add_methods(&mut UserDataMethodsProxy::<_, T, _>::new(methods));
    }

    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        T::add_fields(&mut UserDataFieldsProxy::new(fields))
    }
}
