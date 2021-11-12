# Heavy Alchemy - the black arts of transmutation, wrapped for your safe usage and enjoyment.

`hv-alchemy` is a set of traits and types which maintain a global runtime registry (static is not
possible at the moment, but the `linkme` crate might come into play at some point) of trait object
vtables and other type info such as sizes, alignments, destructors, and more. It requires Rust
nightly for the `ptr_metadata`, `unsize`, and `arbitrary_self_types` features.

A few of the things this crate allows you to do:
- At runtime, ask "do we know if this type implements this object-safe trait?"
- Downcast a `dyn AlchemicalAny` to a concrete sized type `T`
- *Dyn*cast a `dyn AlchemicalAny` to an unsized trait object type `dyn Trait`
- Copy and clone types without compile-time `Copy` and `Clone` bounds (requiring runtime-registered
  `Copy` and `Clone` implementations)
- Extract `Send`, `Sync`, `Copy`, and `Clone` constraints which can be used during compile-time and
  are conditionally accessed during run-time, for specializing behavior according to traits some
  type implements
- Extend the `TypeTable` (vtable registry) for any applicable type at any time with any trait it is
  statically known to implement
- Access "types" at runtime by casting `Type<T>` to `Box<dyn AlchemicalAny>` or any other trait you
  implement for it
- Access the `Default::default` function pointer item as a `fn() -> T` for some type which
  implements `Default` (checking at runtime and without a compile-time `Default` bound)
- Given a `*const dyn AlchemicalAny`, try to clone it into a fresh allocation using the `Layout`
  stored in the type table provided by `dyn AlchemicalAny`

Please use irresponsibly. `hv-alchemy` is `no_std` compatible, but requires `alloc`.

## Caveats

We cannot currently at compile-time register all the different things that some type implements w/
the Alchemy registry. As such you basically have to supply some kind of hook in your APIs (if you're
relying on Alchemy) which encourages/provides a way to add relevant trait objects to the
`TypeTable`s you're interested in. Something like `linkme` or the currently broken `inventory` crate
are capable of doing something somewhat like this but *they cannot universally quantify over types*,
as that would require monomorphizing a `forall T.` bound into a bunch of
`Type::<T>::of().add::<...>()` calls, and for very good reasons which may or may not be obvious to
you, Rust currently has no way to do this.

## How it works

Alchemy keeps a global static `HashMap` of `TypeId`s to `&'static TypeTable`s, which are created
through `Box::leak` (at some point this might be switched to a global `Bump` arena or similar.)
These `TypeTable`s contain again, `HashMap`s of `TypeId` to `&'static DynVtable`s; a `DynVtable`
represents the vtable of some trait object `dyn SomeTrait` for some type `SomeType`. Specifically,
if you want to see if some object implements `fmt::Debug`, Alchemy lookups go something like this:

- Get the `TypeTable` of the object. If we're trying to dyncast a trait object `dyn AlchemicalAny`,
  it's as easy as calling the `.type_table()` method. Otherwise, we could use `Type::<T>::of()`, if
  know `T` statically (and just don't want a static `fmt::Debug` bound.)
  - If we use `Type::<T>::of()`, Alchemy takes the `TypeId` of `T` and uses it to look for its
    `TypeTable` in the global registry. If it's not there, it creates an empty one.
- Take the `TypeId` of `dyn fmt::Debug`, and use that to look for the corresponding `DynVtable` in
  the `vtables` map of the `TypeTable`.
- If we find a `DynVtable` there, we know by invariants that its implementor type will be the type
  of the thing we're trying to check, and that its trait object type will be `dyn fmt::Debug`. We
  can then assume it is safe to use `DynVtable::to_dyn_object_pointer` on our original
  reference/value to convert it to an `&dyn fmt::Debug`. Or if all we wanted to know was whether it
  *did* implement `Debug`, we have our answer.

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
