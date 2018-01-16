# Cargo Travis

Record total test coverage across in-crate and external tests, and upload to [coveralls.io](https://coveralls.io).

The goal is to eventually have feature parity with the assumed-dead [travis-cargo](https://github.com/huonw/travis-cargo)

To avoid problems like [this one](https://github.com/huonw/travis-cargo/pull/55), we link against the cargo crate directly and use its low-level operations. This should be much more reliable than the stdout capture approach. On the other hand, the cargo crate isn't stable, leading to things like [this](https://github.com/roblabla/cargo-travis/issues/1).

## Installation

```
cargo install cargo-travis
export PATH=$HOME/.cargo/bin:$PATH
```

## Example

A possible `travis.yml` configuration is:

```yaml
sudo: false
language: rust

# Cache cargo symbols for faster build
cache: cargo

# Dependencies of kcov, used by coverage
addons:
  apt:
    packages:
      - libcurl4-openssl-dev
      - libelf-dev
      - libdw-dev
      - binutils-dev
      - cmake # also required for cargo-update
    sources:
      - kalakris-cmake

# run builds for all the trains (and more)
rust:
  - nightly
  - beta
  # check it compiles on the latest stable compiler
  - stable
  # and the first stable one (this should be bumped as the minimum
  # Rust version required changes)
  - 1.0.0

before_script:
  - export PATH=$HOME/.cargo/bin:$PATH
  - cargo install cargo-update || echo "cargo-update already installed"
  - cargo install cargo-travis || echo "cargo-travis already installed"
  - cargo install-update -a # update outdated cached binaries

# the main build
script:
  - |
      cargo build &&
      cargo test &&
      cargo bench &&
      cargo doc

after_success:
# measure code coverage and upload to coveralls.io
  - cargo coveralls
```

See the [cargo-update repository](https://github.com/nabijaczleweli/cargo-update) for details on `cargo-update`.

## Help

### `coverage`

```
Record coverage of `cargo test`, this runs all binaries that `cargo test` runs
but not doc tests. The results of all tests are merged into a single directory

Usage:
    cargo coverage [options] [--] [<args>...]

Coverage Options:
    -m PATH, --merge-into PATH   Path to the directory to put the final merged
                                 kcov result into [default: target/kcov]
Test Options:
    -h, --help                   Print this message
    --lib                        Test only this package's library
    --bin NAME                   Test only the specified binary
    --test NAME                  Test only the specified integration test target
    -p SPEC, --package SPEC ...  Package to run tests for
    -j N, --jobs N               Number of parallel jobs, defaults to # of CPUs
    --release                    Build artifacts in release mode, with optimizations
    --features FEATURES          Space-separated list of features to also build
    --all-features               Build all available features
    --no-default-features        Do not build the `default` feature
    --target TRIPLE              Build for the target triple
    --manifest-path PATH         Path to the manifest to build tests for
    -v, --verbose ...            Use verbose output
    -q, --quiet                  No output printed to stdout
    --color WHEN                 Coloring: auto, always, never
    --no-fail-fast               Run all tests regardless of failure
    --frozen                     Require Cargo.lock and cache are up to date
    --locked                     Require Cargo.lock is up to date
```

### `coveralls`

```
Record coverage of `cargo test`, this runs all binaries that `cargo test` runs
but not doc tests. The results of all tests are sent to coveralls.io

Usage:
    cargo coveralls [options] [--] [<args>...]

Test Options:
    -h, --help                   Print this message
    --lib                        Test only this package's library
    --bin NAME                   Test only the specified binary
    --test NAME                  Test only the specified integration test target
    -p SPEC, --package SPEC ...  Package to run tests for
    -j N, --jobs N               Number of parallel jobs, defaults to # of CPUs
    --release                    Build artifacts in release mode, with optimizations
    --features FEATURES          Space-separated list of features to also build
    --all-features               Build all available features
    --no-default-features        Do not build the `default` feature
    --target TRIPLE              Build for the target triple
    --manifest-path PATH         Path to the manifest to build tests for
    -v, --verbose ...            Use verbose output
    -q, --quiet                  No output printed to stdout
    --color WHEN                 Coloring: auto, always, never
    --no-fail-fast               Run all tests regardless of failure
    --frozen                     Require Cargo.lock and cache are up to date
    --locked                     Require Cargo.lock is up to date
```
