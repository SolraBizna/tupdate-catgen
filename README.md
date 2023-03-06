This is a simple utility to generate catalog files for the [Tejat Updater][1].

[1]: https://github.com/SolraBizna/tupdate

# Usage

Install or build this utility. If you already have a Rust environment, this is as simple as doing:

```sh
cargo install tupdate-catgen
```

Navigate to the location of the catalog file you want to create, run `tupdate-catgen` with appropriate parameters, and redirect the output to a catalog file.

```sh
# Example 1: Catalog the whole "many_files" directory and a few zipfiles.
tupdate-catgen -r many_files archive1.zip archive2.zip > main.cat

# Example 2: Catalog a few JPEGs.
tupdate-catgen intro.jpeg title.jpeg outro.jpeg > titlecards.cat
```

# Legalese

The Tejat Updater Catalog Generator is copyright 2023, Solra Bizna, and
licensed under either of:

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or
   <http://www.apache.org/licenses/LICENSE-2.0>)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the Tejat Updater Catalog Generator crate by you, as
defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
