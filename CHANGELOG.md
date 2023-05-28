# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This project uses [_towncrier_](https://towncrier.readthedocs.io/) and the changes for the upcoming release can be found in <https://github.com/vlaci/lzallright/tree/main/changelog.d/>.

<!-- --8<-- [start:changelog] -->

<!-- towncrier release notes start -->

## [v0.2.2](https://github.com/vlaci/lzallright/tree/vv0.2.2) - 2023-05-28

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
