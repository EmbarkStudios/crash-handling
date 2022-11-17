<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->
## [Unreleased] - ReleaseDate
### Changed
- [PR#63](https://github.com/EmbarkStudios/crash-handling/pull/63) added `SadnessFlavor::Abort` to Windows, and `raise_abort` now uses `libc::abort` instead of `std::process::abort` to have consistent behavior between all targets.

### Fixed
- [PR#63](https://github.com/EmbarkStudios/crash-handling/pull/63) fixed compilation for `aarch64-pc-windows-msvc`.

## [0.4.2] - 2022-09-02
### Changed
- [PR#52](https://github.com/EmbarkStudios/crash-handling/pull/52) changed the address used to produce a `sigsegv` to be a canonical rather than a non-canonical address on 64-bit architectures. We will eventually add back support for non-canonical addresses.

## [0.4.1] - 2022-07-21
### Added
- [PR#47](https://github.com/EmbarkStudios/crash-handling/pull/47) added support for raising `EXC_GUARD` exceptions on MacOS.

## [0.4.0] - 2022-07-19
### Changed
- [PR#39](https://github.com/EmbarkStudios/crash-handling/pull/39) resolved [#24](https://github.com/EmbarkStudios/crash-handling/issues/24) and [#37](https://github.com/EmbarkStudios/crash-handling/issues/37) by changing stack overflow to use recursion instead of a single large stack allocated buffer, and changed all crash functions to be `-> !`.

## [0.3.1] - 2022-05-25
### Changed
- Updated to `minidump-writer` 0.2.1 which includes support for MacOS thread names, and aligns on crash-context 0.3.0.

## [0.3.0] - 2022-05-23
### Added
- First usable release of `crash-context`, `crash-handler`, `sadness-generator`, and `minidumper` crates.

## [crash-handler-v0.1.0] - 2022-04-29
### Added
- Initial publish of crash-handler with Linux, Windows, and MacOS support

## [sadness-generator-v0.1.0] - 2022-04-29
### Added
- Initial published of sadness-generator, can generated crashes on Linux, Windows, and MacOS

## [crash-context-v0.2.0] - 2022-04-29
### Added
- Add Windows and MacOS support

## [crash-context-v0.1.0] - 2022-04-21
### Added
- Initial pass of crash-context, Linux only

<!-- next-url -->
[Unreleased]: https://github.com/EmbarkStudios/crash-handling/compare/sadness-generator-0.4.2...HEAD
[0.4.2]: https://github.com/EmbarkStudios/crash-handling/compare/sadness-generator-0.4.1...sadness-generator-0.4.2
[0.4.1]: https://github.com/EmbarkStudios/crash-handling/compare/sadness-generator-0.4.0...sadness-generator-0.4.1
[0.4.0]: https://github.com/EmbarkStudios/crash-handling/compare/0.3.1...sadness-generator-0.4.0
[0.3.1]: https://github.com/EmbarkStudios/crash-handling/compare/0.3.1...0.3.1
[0.3.0]: https://github.com/EmbarkStudios/crash-handling/compare/crash-handler-v0.1.0...0.3.0
[crash-handler-v0.1.0]: https://github.com/EmbarkStudios/crash-handling/releases/tag/crash-handler-v0.1.0
[sadness-generator-v0.1.0]: https://github.com/EmbarkStudios/crash-handling/releases/tag/sadness-generator-v0.1.0
[crash-context-v0.2.0]: https://github.com/EmbarkStudios/crash-handling/releases/tag/crash-context-v0.2.0
[crash-context-v0.1.0]: https://github.com/EmbarkStudios/crash-handling/releases/tag/crash-context-v0.1.0
