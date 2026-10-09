# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- --8<-- [start:changelog] -->

The changelog of the Rust crate is available [here](/changelog-rust).

## Unreleased

### Added

- Built `abi3t` wheels, supporting the stable ABI for free-threaded wheels compatible with Python 3.15t and future versions https://peps.python.org/pep-0803/

## [0.3.0]https://github.com/vlaci/lzallright/tree/v0.3.0) - 2026-10-07

### Changed

- Reimplemented the C++ lzokay based code in Safe Rust. Building no longer needs a C++ toolchain. The codec is also published as the separately versioned `lzallright` Rust crate; see its [changelog](https://vlaci.github.io/lzallright/changelog-rust/) for details.
- Updated PyO3 to 0.29; MSRV is bumped to 1.83
- Minimum supported Python version is now 3.11. Free-threaded wheels are built for 3.14t. x86_64 macOS wheels are no longer published.
- Published wheels are now built with Profile-Guided Optimization, trained on the benchmark corpus.

### Fixed

- Compressing empty input now produces a valid LZO stream; previously it could not be decompressed.
- Inputs other than `bytes` are copied before the GIL is released. Writing to the source buffer during compression or decompression, for example through the `bytearray` behind a read-only `memoryview`, could previously corrupt the result.

## [0.2.6](https://github.com/vlaci/lzallright/tree/v0.2.6) - 2025-06-27


### Added

- Experimental free-thredaing (nogil) support [#95](https://github.com/vlaci/lzallright/pull/95)


### Changed

- Switched to uv project manager from PDM [#93](https://github.com/vlaci/lzallright/pull/93)
  - Dependency updates
  - MSRV is bumped to rustc 1.66

## [0.2.5](https://github.com/vlaci/lzallright/tree/v0.2.5) - 2024-11-18


### Changed

- Updated PyO3 to 0.23 [#61](https://github.com/vlaci/lzallright/issues/61)

## [0.2.4](https://github.com/vlaci/lzallright/tree/v0.2.4) - 2023-12-15


### Fixed

- Fix build from sdist-archive [#44](https://github.com/vlaci/lzallright/issues/44)


## [0.2.3](https://github.com/vlaci/lzallright/tree/v0.2.3) - 2023-05-29


### Added

- Added API documentation [#19](https://github.com/vlaci/lzallright/issues/19)


### Fixed

- Actually statically link to C++ runtime on Linux [#25](https://github.com/vlaci/lzallright/issues/25)


## [0.2.2](https://github.com/vlaci/lzallright/tree/v0.2.2) - 2023-05-28

No significant changes.


## [0.2.1](https://github.com/vlaci/lzallright/tree/v0.2.1) - 2023-05-22


### Fixed

- Fixed homepage URL [#16](https://github.com/vlaci/lzallright/issues/16)


## [0.2.0](https://github.com/vlaci/lzallright/tree/v0.2.0) - 2023-05-22


### Added

- Decompressed data is available on the exception object when there is trailing garbage in input [#8](https://github.com/vlaci/lzallright/issues/8)
- Exceptions are now importable [#8](https://github.com/vlaci/lzallright/issues/8)
- Added possibility to give an output size hint for decompression [#12](https://github.com/vlaci/lzallright/issues/12)


### Changed

- Build with Maturin 0.15 [#11](https://github.com/vlaci/lzallright/issues/11)


### Fixed

- Fixed build to actually statically link against C++ runtime on Linux [#12](https://github.com/vlaci/lzallright/issues/12)
- Fixed busy loop during decompression when compression ratio is bigger than 400% [#12](https://github.com/vlaci/lzallright/issues/12)
