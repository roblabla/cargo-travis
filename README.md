# Cargo Travis

Record total test coverage across in-crate and external tests, and upload to [coveralls.io](https://coveralls.io).

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
# Dependencies of kcov, used by coverage
addons:
  apt:
    packages:
      - libcurl4-openssl-dev
      - libelf-dev
      - libdw-dev
      - binutils-dev

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
  - |
      cargo install cargo-travis &&
      export PATH=$HOME/.cargo/bin:$PATH

# the main build
script:
  - |
      cargo build &&
      cargo test &&
      cargo bench &&
      cargo --only stable doc

after_success:
  # measure code coverage and upload to coveralls.io (the verify
  # argument mitigates kcov crashes due to malformed debuginfo, at the
  # cost of some speed <https://github.com/huonw/travis-cargo/issues/12>)
  - cargo coveralls --verify
```
