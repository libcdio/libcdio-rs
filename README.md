# libcdio-rs
Safe wrapper and Rust port of [GNU libcdio][libcdio-site].

For raw bindings, check out [libcdio-sys][libcdio-sys].

## Usage
- Install [clang][bindgen-reqs].
- Add with cargo:
  ```shell
  cargo add libcdio-rs --all-features
  ```

Feature flags are provided and guard the modules named after them.
See the [documentation][docs].

## Design
Currently, this library is partly a safe wrapper over libcdio and
partly a port, with the goal of an eventual transition to a full
rewrite.

Progress so far:
- Ported SCSI MMC functionality.
- Implemented safe wrappers for a subset of ISO 9660 and UDF routines.

## SCSI MMC references
This library implements SCSI routines primarily based on
[MMC-6 rev.2g][mmc6r2g] and [SPC-3 rev.23][spc3r23].

## License
Copyright (C) 2026 Shiva Kiran Koninty <shiva@skran.xyz>

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by the
Free Software Foundation, either version 3 of the License, or (at your
option) any later version.

This program is distributed in the hope that it will be useful, but
WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program. If not, see <https://www.gnu.org/licenses/>.

[libcdio-site]: https://libcdio.github.io
[libcdio-sys]: https://crates.io/crates/libcdio-sys
[bindgen-reqs]: https://rust-lang.github.io/rust-bindgen/requirements.html
[docs]: https://docs.rs/libcdio-rs/latest/libcdio-rs
[mmc6r2g]: https://www.13thmonkey.org/documentation/SCSI/mmc6r02g.pdf
[spc3r23]: https://www.13thmonkey.org/documentation/SCSI/spc3r23.pdf
