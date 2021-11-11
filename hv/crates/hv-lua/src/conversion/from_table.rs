use std::ops::{Deref, DerefMut};

use crate::{Error, FromLua, Lua, Result, ToLua, Value};

pub type Sequence<T> = FromTable<Vec<T>>;

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FromTable<T>(pub T);

impl<T> Deref for FromTable<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for FromTable<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'lua, T: ToLua<'lua>> ToLua<'lua> for FromTable<Vec<T>> {
    fn to_lua(self, lua: &'lua Lua) -> Result<Value<'lua>> {
        Ok(Value::Table(lua.create_sequence_from(self.0)?))
    }
}

impl<'lua, T: FromLua<'lua>> FromLua<'lua> for FromTable<Vec<T>> {
    fn from_lua(value: Value<'lua>, _: &'lua Lua) -> Result<Self> {
        if let Value::Table(table) = value {
            table
                .sequence_values()
                .collect::<Result<Vec<T>>>()
                .map(Self)
        } else {
            Err(Error::FromLuaConversionError {
                from: value.type_name(),
                to: "Sequence",
                message: Some("expected table".to_string()),
            })
        }
    }
}

impl<T: IntoIterator> IntoIterator for FromTable<T> {
    type IntoIter = T::IntoIter;
    type Item = T::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<T: FromIterator<A>, A> FromIterator<A> for FromTable<T> {
    fn from_iter<I: IntoIterator<Item = A>>(iter: I) -> Self {
        Self(T::from_iter(iter))
    }
}
