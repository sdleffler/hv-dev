# Heavy Guarded-Borrow - traits for generalizing over guarded borrowing operations

This crate requires nightly and relies on the unstable feature `generic_associated_types`. It
provides traits describing borrowing patterns (immutable borrows, mutable borrows from immutable
references, and mutable borrows from mutable references) with accompanying associated `Guard` types
parameterized by lifetime.

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