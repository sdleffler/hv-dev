# Heavy Lease-Tracker - tracking of borrow origins for diagnostics of runtime borrow failures

`LeaseTracker` implements a simple system using guards and the Rust `core::panic::Location` API to
track and provide information on the source locations of borrows for when interior mutability
primitives you write fail.

`no-std` compatible; uses `alloc` for `Arc` and spinlocks for very short critical sections.

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