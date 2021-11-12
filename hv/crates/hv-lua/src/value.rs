use std::{iter, mem, slice, str, vec};

#[cfg(feature = "serialize")]
use {
    serde::ser::{self, Serialize, Serializer},
    std::result::Result as StdResult,
};

use crate::error::{Error, Result};
use crate::function::Function;
use crate::lua::Lua;
use crate::string::String;
use crate::table::Table;
use crate::thread::Thread;
use crate::types::{Integer, LightUserData, Number};
use crate::userdata::AnyUserData;

/// A dynamically typed Lua value. The `String`, `Table`, `Function`, `Thread`, and `UserData`
/// variants contain handle types into the internal Lua state. It is a logic error to mix handle
/// types between separate `Lua` instances, and doing so will result in a panic.
#[derive(Debug, Clone)]
pub enum Value<'lua> {
    /// The Lua value `nil`.
    Nil,
    /// The Lua value `true` or `false`.
    Boolean(bool),
    /// A "light userdata" object, equivalent to a raw pointer.
    LightUserData(LightUserData),
    /// An integer number.
    ///
    /// Any Lua number convertible to a `Integer` will be represented as this variant.
    Integer(Integer),
    /// A floating point number.
    Number(Number),
    /// An interned string, managed by Lua.
    ///
    /// Unlike Rust strings, Lua strings may not be valid UTF-8.
    String(String<'lua>),
    /// Reference to a Lua table.
    Table(Table<'lua>),
    /// Reference to a Lua function (or closure).
    Function(Function<'lua>),
    /// Reference to a Lua thread (or coroutine).
    Thread(Thread<'lua>),
    /// Reference to a userdata object that holds a custom type which implements `UserData`.
    /// Special builtin userdata types will be represented as other `Value` variants.
    UserData(AnyUserData<'lua>),
    /// `Error` is a special builtin userdata type. When received from Lua it is implicitly cloned.
    Error(Error),
}

pub use self::Value::Nil;

impl<'lua> Value<'lua> {
    pub fn type_name(&self) -> &'static str {
        match *self {
            Value::Nil => "nil",
            Value::Boolean(_) => "boolean",
            Value::LightUserData(_) => "lightuserdata",
            Value::Integer(_) => "integer",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Table(_) => "table",
            Value::Function(_) => "function",
            Value::Thread(_) => "thread",
            Value::UserData(_) => "userdata",
            Value::Error(_) => "error",
        }
    }

    /// Compares two values for equality.
    ///
    /// Equality comparisons do not convert strings to numbers or vice versa.
    /// Tables, Functions, Threads, and Userdata are compared by reference:
    /// two objects are considered equal only if they are the same object.
    ///
    /// If Tables or Userdata have `__eq` metamethod then mlua will try to invoke it.
    /// The first value is checked first. If that value does not define a metamethod
    /// for `__eq`, then mlua will check the second value.
    /// Then mlua calls the metamethod with the two values as arguments, if found.
    pub fn equals<T: AsRef<Self>>(&self, other: T) -> Result<bool> {
        match (self, other.as_ref()) {
            (Value::Table(a), Value::Table(b)) => a.equals(b),
            (Value::UserData(a), Value::UserData(b)) => a.equals(b),
            _ => Ok(self == other.as_ref()),
        }
    }
}

impl<'lua> PartialEq for Value<'lua> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::LightUserData(a), Value::LightUserData(b)) => a == b,
            (Value::Integer(a), Value::Integer(b)) => *a == *b,
            (Value::Integer(a), Value::Number(b)) => *a as Number == *b,
            (Value::Number(a), Value::Integer(b)) => *a == *b as Number,
            (Value::Number(a), Value::Number(b)) => *a == *b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Table(a), Value::Table(b)) => a == b,
            (Value::Function(a), Value::Function(b)) => a == b,
            (Value::Thread(a), Value::Thread(b)) => a == b,
            (Value::UserData(a), Value::UserData(b)) => a == b,
            _ => false,
        }
    }
}

impl<'lua> AsRef<Value<'lua>> for Value<'lua> {
    #[inline]
    fn as_ref(&self) -> &Self {
        self
    }
}

#[cfg(feature = "serialize")]
impl<'lua> Serialize for Value<'lua> {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Value::Nil => serializer.serialize_unit(),
            Value::Boolean(b) => serializer.serialize_bool(*b),
            #[allow(clippy::useless_conversion)]
            Value::Integer(i) => serializer.serialize_i64((*i).into()),
            #[allow(clippy::useless_conversion)]
            Value::Number(n) => serializer.serialize_f64((*n).into()),
            Value::String(s) => s.serialize(serializer),
            Value::Table(t) => t.serialize(serializer),
            Value::UserData(ud) => ud.serialize(serializer),
            Value::LightUserData(ud) if ud.0.is_null() => serializer.serialize_none(),
            Value::Error(_) | Value::LightUserData(_) | Value::Function(_) | Value::Thread(_) => {
                let msg = format!("cannot serialize <{}>", self.type_name());
                Err(ser::Error::custom(msg))
            }
        }
    }
}

/// Trait for types convertible to `Value`.
pub trait ToLua<'lua> {
    /// Performs the conversion.
    fn to_lua(self, lua: &'lua Lua) -> Result<Value<'lua>>;
}

/// Trait for types convertible from `Value`.
pub trait FromLua<'lua>: Sized {
    /// Performs the conversion.
    fn from_lua(lua_value: Value<'lua>, lua: &'lua Lua) -> Result<Self>;
}

/// Multiple Lua values used for both argument passing and also for multiple return values.
#[derive(Debug)]
pub struct MultiValue<'lua> {
    // Values are kept in reverse order, making it easier to pop Lua values off of the stack and put
    // them directly into the multivalue using `Vec::push` (see `Function::call`, for n results on
    // the stack, lua.pop_value() => Vec::push happens n times. Is this a curiosity of what order
    // Lua return values exist on the Lua stack?)
    stack: Vec<Value<'lua>>,
    lua: &'lua Lua,
}

impl<'lua> Clone for MultiValue<'lua> {
    fn clone(&self) -> Self {
        let mut new_stack = self.lua.pull_multivalue_vec();
        new_stack.clone_from(&self.stack);
        MultiValue {
            stack: new_stack,
            lua: self.lua,
        }
    }
}

impl<'lua> MultiValue<'lua> {
    /// Creates an empty `MultiValue` containing no values.
    #[inline]
    pub fn new(lua: &'lua Lua) -> MultiValue<'lua> {
        MultiValue {
            stack: lua.pull_multivalue_vec(),
            lua,
        }
    }
}

impl<'lua> IntoIterator for MultiValue<'lua> {
    type Item = Value<'lua>;
    type IntoIter = MultiValueIntoIter<'lua>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        MultiValueIntoIter(self)
    }
}

impl<'a, 'lua> IntoIterator for &'a MultiValue<'lua> {
    type Item = &'a Value<'lua>;
    type IntoIter = iter::Rev<slice::Iter<'a, Value<'lua>>>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        (&self.stack).iter().rev()
    }
}

impl<'lua> MultiValue<'lua> {
    #[inline]
    pub fn from_iter<I>(iter: I, lua: &'lua Lua) -> MultiValue<'lua>
    where
        I: IntoIterator<Item = Value<'lua>>,
        I::IntoIter: DoubleEndedIterator,
    {
        let mut stack = lua.pull_multivalue_vec();
        stack.extend(iter.into_iter().rev());
        MultiValue { stack, lua }
    }

    #[inline]
    pub fn try_from_iter<I>(iter: I, lua: &'lua Lua) -> Result<MultiValue<'lua>>
    where
        I: IntoIterator<Item = Result<Value<'lua>>>,
        I::IntoIter: DoubleEndedIterator,
    {
        let mut stack = lua.pull_multivalue_vec();
        for v in iter.into_iter().rev() {
            stack.push(v?);
        }
        Ok(MultiValue { stack, lua })
    }

    #[inline]
    pub fn from_vec(mut v: Vec<Value<'lua>>, lua: &'lua Lua) -> MultiValue<'lua> {
        v.reverse();
        MultiValue { stack: v, lua }
    }

    #[inline]
    pub fn into_vec(mut self) -> Vec<Value<'lua>> {
        let mut v = mem::take(&mut self.stack);
        mem::forget(self);
        v.reverse();
        v
    }

    #[inline]
    pub(crate) fn reserve(&mut self, size: usize) {
        self.stack.reserve(size);
    }

    #[inline]
    pub(crate) fn push_front(&mut self, value: Value<'lua>) {
        self.stack.push(value);
    }

    #[inline]
    pub(crate) fn pop_front(&mut self) -> Option<Value<'lua>> {
        self.stack.pop()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.stack.len() == 0
    }

    #[inline]
    pub fn iter(&self) -> iter::Rev<slice::Iter<Value<'lua>>> {
        self.stack.iter().rev()
    }

    #[inline]
    pub fn drain(&mut self) -> iter::Rev<vec::Drain<'_, Value<'lua>>> {
        self.stack.drain(..).rev()
    }
}

impl<'lua> Drop for MultiValue<'lua> {
    fn drop(&mut self) {
        self.stack.clear();
        self.lua.recycle_multivalue_vec(mem::take(&mut self.stack));
    }
}

#[derive(Debug)]
pub struct MultiValueIntoIter<'lua>(MultiValue<'lua>);

impl<'lua> Iterator for MultiValueIntoIter<'lua> {
    type Item = Value<'lua>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.stack.pop()
    }
}

/// Trait for types convertible to any number of Lua values.
///
/// This is a generalization of `ToLua`, allowing any number of resulting Lua values instead of just
/// one. Any type that implements `ToLua` will automatically implement this trait.
pub trait ToLuaMulti<'lua> {
    /// Performs the conversion.
    fn to_lua_multi(self, lua: &'lua Lua) -> Result<MultiValue<'lua>>;
}

/// Trait for types that can be created from an arbitrary number of Lua values.
///
/// This is a generalization of `FromLua`, allowing an arbitrary number of Lua values to participate
/// in the conversion. Any type that implements `FromLua` will automatically implement this trait.
pub trait FromLuaMulti<'lua>: Sized {
    /// Performs the conversion.
    ///
    /// In case `values` contains more values than needed to perform the conversion, the excess
    /// values should be ignored. This reflects the semantics of Lua when calling a function or
    /// assigning values. Similarly, if not enough values are given, conversions should assume that
    /// any missing values are nil.
    fn from_lua_multi(values: MultiValue<'lua>, lua: &'lua Lua) -> Result<Self>;
}
