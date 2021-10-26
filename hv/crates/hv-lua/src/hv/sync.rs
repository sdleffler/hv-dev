use hv_sync::cell::ArcCell;

use crate::{
    types::{MaybeSend, MaybeSync},
    UserData, UserDataMethods,
};

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
