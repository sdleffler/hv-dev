# Heavy ECS - `hecs` shim

This crate exists to provide a shim between `hecs` and Heavy, for the reason that at current Heavy
depends on several extensions to `hecs` we hope to upstream soon. Until that happens, this crate
contains direct copy of our fork of the `hecs` source code, subject to the same license as `hecs`
itself.

Once the fork is upstreamed this crate will become a single module with a single line that reexports
`hecs`.

This is not expected to be used outside of Heavy.
