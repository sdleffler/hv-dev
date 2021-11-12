# Heavy Elastic - safe lifetime stretching abstractions, for the adventurous or morbidly curious soul

Ever needed to access an `&'a T` from a closure which has to be `'static`?

Know that you could "loan" that `&'a T` for just the times that that closre will run?

If so, then the `Elastic` type is for you! Please use responsibly. `no-std` compatible. Requires
nightly for features `generic_associated_types`. Do not use during a new moon. During a blood
moon or solar eclipse, refrigerate any machines which contain the source code for this crate and
keep them under lock and key.

## Features

- "Loaning" immutable or mutable references into `'static` code, with a non-`'static` guard to
  ensure the references do not live past their lifetime.
- `Elastic` acts as a shared reference with refcell-like internals, providing easy interior
  mutability.
- May eat your dog. Will not eat cats. It likes cats. So do I.
- This crate is safe for very specific use cases; please see the documentaiton for the `Stretched`
  trait for more info. There are very strict requirements on implementing `Stretched`. If you
  blindly implement `Stretched` in a way which violates these requirements, you are going to be in
  instant undefined-behavior-land.

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