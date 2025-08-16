# Changelog

## [0.36.0](https://github.com/fornwall/rust-script/releases/tag/0.36.0) 2025-08-16
### Fixed
- Fix issue with `--clear-cache` when "projects" directory doesn't exist ([#152](https://github.com/fornwall/rust-script/pull/152)).

### Added
- Allow passing additional arguments to `cargo test` and `cargo bench` ([#146](https://github.com/fornwall/rust-script/pull/146)).

### Internal
- Update dependencies.

## [0.35.0](https://github.com/fornwall/rust-script/releases/tag/0.35.0) 2024-09-03
### Fixed
- Make `RUST_SCRIPT_BASE_PATH` report the correct path when `rust-script` executes with `--base-path` ([#136](https://github.com/fornwall/rust-script/pull/136)).
- Bump dependencies, raising MSRV from `1.64` to `1.74` ([#138](https://github.com/fornwall/rust-script/pull/138)).

## [0.34.0](https://github.com/fornwall/rust-script/releases/tag/0.34.0) 2023-09-27
### Added
- Publish binaries on GitHub releases, for use with e.g. `cargo binstall`.
