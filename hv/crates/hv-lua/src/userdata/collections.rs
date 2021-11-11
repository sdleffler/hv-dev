use hv_alchemy::{Type, WithClone, WithSend, WithSync};

use crate::{types::MaybeSend, FromLua, MetaMethod, ToLua, UserData, UserDataMethods};

struct VecProxy<'m, T, M> {
    methods: &'m mut M,
    table: Type<Vec<T>>,
    element: Type<T>,
}

impl<'m, T: 'static, M> WithSend<T> for VecProxy<'m, T, M> {
    fn with_send(&mut self)
    where
        T: Send,
    {
        self.table.add_send();
    }
}

impl<'m, T: 'static, M> WithSync<T> for VecProxy<'m, T, M> {
    fn with_sync(&mut self)
    where
        T: Sync,
    {
        self.table.add_sync();
    }
}

impl<'m, 'lua, T, M> WithClone<T> for VecProxy<'m, T, M>
where
    T: for<'l> FromLua<'l> + for<'l> ToLua<'l> + 'static,
    M: UserDataMethods<'lua, Vec<T>>,
{
    fn with_clone(&mut self)
    where
        T: Clone,
    {
        struct Wrap<T>(T);

        self.table.add_clone();

        impl<'a, 'm, 'lua, T, M> WithSend<T> for Wrap<&'a mut VecProxy<'m, T, M>>
        where
            T: for<'l> FromLua<'l> + for<'l> ToLua<'l> + Clone + 'static,
            M: UserDataMethods<'lua, Vec<T>>,
        {
            fn with_send(&mut self)
            where
                T: Send,
            {
                let methods = &mut *self.0.methods;
                methods.add_method("clone", |_, this, ()| Ok(this.clone()));
                methods.add_meta_method(MetaMethod::Index, |_, this, idx: usize| {
                    Ok(this.get(idx).cloned())
                });
            }
        }

        self.element.try_prove_send(&mut Wrap(self));
    }
}

impl<T: for<'lua> FromLua<'lua> + for<'lua> ToLua<'lua> + 'static> UserData for Vec<T> {
    #[allow(clippy::unit_arg)]
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        let element = Type::<T>::of();
        let table = Type::<Self>::of();

        {
            let mut proxy = VecProxy {
                methods,
                table,
                element,
            };

            element.try_prove_send(&mut proxy);
            element.try_prove_sync(&mut proxy);
            element.try_prove_clone(&mut proxy);
        }

        methods.add_method_mut("push", |_, this, t| Ok(this.push(t)));
        methods.add_method_mut("pop", |_, this, ()| Ok(this.pop()));

        // Meta-methods
        methods.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.len()));
        methods.add_meta_method_mut(MetaMethod::NewIndex, |_, this, (idx, t): (usize, T)| {
            Ok(this[idx] = t)
        });
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M)
    where
        Self: 'static + MaybeSend,
    {
        methods.add_function("new", |_, ()| Ok(Vec::new()));
        methods.add_function("with_capacity", |_, ()| Ok(Vec::new()));
    }
}
