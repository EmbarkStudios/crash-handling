<!-- Allow this file to not have a first line heading -->
<!-- markdownlint-disable-file MD041 -->

<!-- inline html -->
<!-- markdownlint-disable-file MD033 -->

<div align="center">

# `🔥 crash-handling`

**Set of utility crates for catching and handling crashes**

[![Embark](https://img.shields.io/badge/embark-open%20source-blueviolet.svg)](https://embark.dev)
[![Embark](https://img.shields.io/badge/discord-ark-%237289da.svg?logo=discord)](https://discord.gg/dAuKfZS)
[![Build status](https://github.com/EmbarkStudios/crash-handling/workflows/CI/badge.svg)](https://github.com/EmbarkStudios/crash-handling/actions)
<!-- [![Crates.io](https://img.shields.io/crates/v/rust-gpu.svg)](https://crates.io/crates/rust-gpu) -->
<!-- [![Docs](https://docs.rs/rust-gpu/badge.svg)](https://docs.rs/rust-gpu) -->
<!-- [![dependency status](https://deps.rs/repo/github/EmbarkStudios/rust-gpu/status.svg)](https://deps.rs/repo/github/EmbarkStudios/rust-gpu) -->

</div>

## WIP

- [`crash-context`](crash-context) - Provides portable types containing target specific contextual information at the time of an exception
- [`exception-handler`](exception-handler) - Provides signal/exception handlers that invoke a user supplied callback with the contextual information on the crash
- [`minidumper`](minidumper) - Provides an IPC client and server, the client provides a crash context and the server creates a minidump based on that crash context and invokes a user supplied callback with the minidump

## Contribution

[![Contributor Covenant](https://img.shields.io/badge/contributor%20covenant-v1.4-ff69b4.svg)](CODE_OF_CONDUCT.md)

We welcome community contributions to this project.

Please read our [Contributor Guide](CONTRIBUTING.md) for more information on how to get started.
Please also read our [Contributor Terms](CONTRIBUTING.md#contributor-terms) before you make any contributions.

Any contribution intentionally submitted for inclusion in an Embark Studios project, shall comply with the Rust standard licensing model (MIT OR Apache 2.0) and therefore be dual licensed as described below, without any additional terms or conditions:

### License

This contribution is dual licensed under EITHER OF

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

For clarity, "your" refers to Embark or any other licensee/user of the contribution.
