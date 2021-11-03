# Heavy - an opinionated, efficient, relatively lightweight, and tightly Lua-integrated game framework for Rust

*Slow down, upon the teeth of Orange*

Heavy is a mostly backend/platform-agnostic game framework for Rust, with the target of providing
efficient primitives for game development while also remaining tightly integrated with Lua scripting.

It consists of two main parts:
- The `hv` crate, an aggregation of modular sub-crates which implement a Lua interface and wrap it
  around a number of common utilities (ECS, math, input mapping, etc.) without being tied in
  specific to a rendering strategy.
- Altar, an engine built on `hv` for building top-down 2.5D games, using `luminance` for
  platform-agnostic graphics and supporting multiple underlying windowing libraries.
  
At current there is also a local fork of `hecs` adding support for dynamic queries and exposing some
parts of the `hecs` system which were previously hidden. Hopefully to be upstreamed. The scheduler
(`yaks`) is dependent on `hecs`, so we have a local fork of it too.

## Why?

Current Rust game frameworks don't have first-class support for things like Lua integration, and
integrating a crate like `mlua` with external crates is made complicated by Rust's orphan rules;
it's a compiler error to try to implement `mlua::UserData` for say, `hecs::World`. By forking
`mlua`, we get the ability to add first-class support for the notion of a Rust *type* reified as
userdata (through `hv::lua::Lua::create_type_userdata`.) This is done through the incredibly cursed
`hv-alchemy` crate.

In other words, the goal of Heavy is to provide a Rusty, efficient interface, which is also tightly
integrated with scripting for fast iteration and moddability down the road, as well as for working
with non-coding artists and less-technical team members who find working with a scripting language
like Lua easier to deal with than a language with a high learning curve and domain knowledge
requirement (Rust.)

```
Also, I'm an idiot, so I like making game frameworks/engines.
- sleff
```

### `hv` - Features

These are all in progress/goals:

- `hecs`-based ECS with `yaks`-based executor/system scheduler.
- Lua integration based on a custom fork of `mlua`, providing easy and powerful integration with
  Rust traits and types thanks to `hv-alchemy`.
  - `hv-` crates integrated w/ Lua by default, w/ runtime reflection support for creating and
    manipulating Rust types from Lua with minimal (but present) boilerplate:
    - `hv-math` (`nalgebra` and goodies)
    - `hecs` (ECS, entity spawning and querying)
    - `hv-filesystem` (virtual filesystem)
    - `hv-alchemy` (runtime trait object registration and manipulation)
    - `hv-input` (input mappings and state)
  - Support for "Rust type userdata objects" through `hv-alchemy` and Alchemical reflection on
    `AnyUserData` objects.
- Synchronization primitives and other goodies useful for interfacing with Lua.
- (TODO) audio through FMOD.
- Portability limited only by the Rust standard library and Lua (and eventually FMOD).

### `altar` - Features

- Implemented with `hv` at its core.
- Abstracted external events.
- Abstracted rendering provided by `luminance`.
- Portability limited only by the Rust standard library, Lua, and luminance.

## Motivating example: defining a Lua interface for a component type, spawning entities in and querying the ECS from Lua

Connecting a component type w/ `hv` is done through the `UserData` trait. Here's a contrived but
ultra-simple example implementation for a component which just wraps an `i32`:

```rust
/// A component type wrapping an `i32`; for technical reasons, primitives cannot be viewed as
/// components from Lua (because they can't implement `UserData`.)
#[derive(Debug, Clone, Copy)]
struct I32Component(i32);

// The `UserData` impl defines how the type interacts with Lua and also what methods its type object
// has available.
impl UserData for I32Component {
    // We allow access to the internal value via a Lua field getter/setter pair.
    #[allow(clippy::unit_arg)]
    fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("value", |_, this| Ok(this.0));
        fields.add_field_method_set("value", |_, this, value| Ok(this.0 = value));
    }

    // Rust simply does not have compile-time reflection. The `on_metatable_init` method provides
    // the ability to register traits we need at run-time for this type; it also doubles as a way of
    // requiring Rust to generate the code for the vtables of those traits (which would not
    // otherwise happen if they were not actually used.) `.mark_component()` comes from the
    // `LuaUserDataTypeExt` trait which provides convenient shorthand for registering required
    // traits; in this case, `mark_component` registers `dyn Send` and `dyn Sync` impls which are
    // sufficient to act as a component.
    fn on_metatable_init(t: Type<Self>) {
        t.add_clone().add_copy().mark_component();
    }

    // The following methods are a bit like implementing `UserData` on `Type<Self>`, the userdata
    // type object of `Self`. This one just lets you construct an `I32Component` from Lua given a
    // value convertible to an `i32`.
    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, i: i32| Ok(Self(i)));
    }

    // We want to generate the necessary vtables for accessing this type as a component in the ECS.
    // The `LuaUserDataTypeTypeExt` extension trait provides convenient methods for registering the
    // required traits for this (`.mark_component_type()` is shorthand for
    // `.add::<dyn ComponentType>()`.)
    fn on_type_metatable_init(t: Type<Type<Self>>) {
        t.mark_component_type();
    }
}
```

Now given this, we can write something like this:

```rust
// Create a Lua context.
let lua = Lua::new();
// Load some builtin `hv` types into the Lua context in a global `hv` table (this is going to 
// change; I'd like a better way to do this)
let hv = hv::lua::types(&lua)?;

// Create userdata type objects for the `I32Component` defined above as well as a similarly defined
// `BoolComponent` (exercise left to the reader)
let i32_ty = lua.create_userdata_type::<I32Component>()?;
let bool_ty = lua.create_userdata_type::<BoolComponent>()?;

// To share an ECS world between Lua and Rust, we'll need to wrap it in an `Arc<AtomicRefCell<_>>`.
// Heavy provides other potentially more efficient ways to do this sharing but this is sufficient
// for this example.
let world = Arc::new(AtomicRefCell::new(World::new()));
// Clone the world so that it doesn't become owned by Lua. We still want a copy!
let world_clone = world.clone();

// `chunk` macro allows for in-line Lua definitions w/ quasiquoting for injecting values from Rust.
let chunk = chunk! {
    // Drag in the `hv` table we created above, and also the `I32Component` and `BoolComponent` types,
    // presumptuously calling them `I32` and `Bool` just because they're wrappers around the fact we
    // can't just slap a primitive in there and call it a day.
    local hv = $hv
    local Query = hv.ecs.Query
    local I32, Bool = $i32_ty, $bool_ty

    local world = $world_clone
    // Spawn an entity, dynamically adding components to it taken from userdata! Works with copy,
    // clone, *and* non-clone types (non-clone types will be moved out of the userdata and the userdata
    // object marked as destructed)
    local entity = world:spawn { I32.new(5), Bool.new(true) }
    // Dynamic query functionality, using our fork's `hecs::DynamicQuery`.
    local query = Query.new { Query.write(I32), Query.read(Bool) }
    // Querying takes a closure in order to enforce scope - the queryitem will panic if used outside that
    // scope.
    world:query_one(query, entity, function(item)
        // Querying allows us to access components of our item as userdata objects through the same interface
        // we defined above!
        assert(item:take(Bool).value == true)
        local i = item:take(I32)
        assert(i.value == 5)
        i.value = 6
        assert(i.value == 6)
    end)

    // Return the entity we spawned back to Rust so we can examine it there.
    return entity
};

// Run the chunk and get the returned entity.
let entity: Entity = lua.load(chunk).eval()?;

// Look! It worked!
let borrowed = world.borrow();
let mut q = borrowed
    .query_one::<(&I32Component, &BoolComponent)>(entity)
    .ok();
assert_eq!(
    q.as_mut().and_then(|q| q.get()).map(|(i, b)| (i.0, b.0)),
    Some((6, true))
);
```

Without `hv`, doing this would require a massive amount of boilerplate for the component types,
wrapping a `hecs::World` in your own custom userdata type that supports this type-based
manipulation since as a foreign type you can't impl `mlua::UserData` directly, wrapping
`hecs::Entity` etc., writing dynamic wrappers around everything required to insert a component of a
given type on a per-component basis (Heavy boils this all down into `.add::<dyn ComponentType>()`),
wrappers for a *borrowed* world, ensuring that the Lua code transparently shows where it borrows and
unborrows the world, etc.

So while there's still some unavoidable boilerplate, it's a lot less of a mess and allows for
writing Lua interaction code as if you're directly interacting with a given type rather than with
your own custom wrapper boilerplate.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
