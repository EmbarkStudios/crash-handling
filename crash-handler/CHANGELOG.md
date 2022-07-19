<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->
## [Unreleased] - ReleaseDate
## [0.3.2] - 2022-07-19
### Fixed
- [PR#38](https://github.com/EmbarkStudios/crash-handling/pull/38) resolved [#31](https://github.com/EmbarkStudios/crash-handling/issues/31) and [#35](https://github.com/EmbarkStudios/crash-handling/issues/35) by adding support for 64-bit codes in the mach exception information, as well as now handling `EXC_CRASH` exceptions.
- [PR#43](https://github.com/EmbarkStudios/crash-handling/pull/42) resolved [#42](https://github.com/EmbarkStudios/crash-handling/issues/42) by fixing a bug on `aarch64-linux`. Thanks [@sfackler](https://github.com/sfackler)!

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
[Unreleased]: https://github.com/EmbarkStudios/crash-handling/compare/crash-handler-0.3.2...HEAD
[0.3.2]: https://github.com/EmbarkStudios/crash-handling/compare/0.3.1...crash-handler-0.3.2
[0.3.1]: https://github.com/EmbarkStudios/crash-handling/compare/0.3.1...0.3.1
[0.3.0]: https://github.com/EmbarkStudios/crash-handling/compare/crash-handler-v0.1.0...0.3.0
[crash-handler-v0.1.0]: https://github.com/EmbarkStudios/crash-handling/releases/tag/crash-handler-v0.1.0
[sadness-generator-v0.1.0]: https://github.com/EmbarkStudios/crash-handling/releases/tag/sadness-generator-v0.1.0
[crash-context-v0.2.0]: https://github.com/EmbarkStudios/crash-handling/releases/tag/crash-context-v0.2.0
[crash-context-v0.1.0]: https://github.com/EmbarkStudios/crash-handling/releases/tag/crash-context-v0.1.0
