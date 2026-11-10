# Bundled Typst asset notices

`LICENSE` and `NOTICE` are copied without modification from the
[`typst-assets` 0.15.1 crate](https://crates.io/crates/typst-assets/0.15.1),
whose source is maintained at <https://github.com/typst/typst-assets>.
The exact dependency is recorded in the root `Cargo.lock`.

The native Python extension embeds the provider's font data through Noon's
`bundled-fonts` feature. The provider's Apache-2.0 license does not replace
the individual asset licenses. Its complete `NOTICE` retains those terms,
including Libertinus, DejaVu, NewComputerModern, and the NewCM10-Regular
font and distribution exceptions.

These files are installed under the wheel's `.dist-info/licenses` directory
and retained in the source distribution. Recheck this directory against the
provider whenever the locked `typst-assets` version changes.
