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
sudo: required
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
# upload documentation to github.io (gh-pages branch)
  - cargo doc-upload
```

See the [cargo-update repository](https://github.com/nabijaczleweli/cargo-update) for details on `cargo-update`.

Note that `sudo: required` is necessary to use kcov. See [this issue](https://github.com/travis-ci/travis-ci/issues/9061) for more information.

## Help

### `coverage`

```
Record coverage of `cargo test`, this runs all binaries that `cargo test` runs
but not doc tests. The results of all tests are merged into a single directory

Usage:
    cargo coverage [options] [--] [<args>...]

Coverage Options:
    -V, --version                Print version info and exit
    -m PATH, --merge-into PATH   Path to the directory to put the final merged
                                 kcov result into [default: target/kcov]
    --exclude-pattern PATTERN    Comma-separated path patterns to exclude from the report
    --kcov-build-location PATH   Path to the directory in which to build kcov (into a new folder)
                                 [default: target] -- kcov ends up in target/kcov-master

Test Options:
    -h, --help                   Print this message
    --lib                        Test only this package's library
    --bin NAME                   Test only the specified binary
    --bins                       Test all binaries
    --test NAME                  Test only the specified integration test target
    --tests                      Test all tests
    --bench NAME ...             Test only the specified bench target
    --benches                    Test all benches
    --all-targets                Test all targets (default)
    -p SPEC, --package SPEC ...  Package to run tests for
    --all                        Test all packages in the workspace
    --exclude SPEC ...           Exclude packages from the test
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
    -Z FLAG ...                  Unstable (nightly-only) flags to Cargo
```

### `coveralls`

```
Record coverage of `cargo test`, this runs all binaries that `cargo test` runs
but not doc tests. The results of all tests are sent to coveralls.io

Usage:
    cargo coveralls [options] [--] [<args>...]

Coveralls Options:
    -V, --version                Print version info and exit
    --exclude-pattern PATTERN    Comma-separated  path patterns to exclude from the report
    --kcov-build-location PATH   Path to the directory in which to build kcov (into a new folder)
                                 [default: target] -- kcov ends up in target/kcov-master

Test Options:
    -h, --help                   Print this message
    --lib                        Test only this package's library
    --bin NAME                   Test only the specified binary
    --bins                       Test all binaries
    --test NAME                  Test only the specified integration test target
    --tests                      Test all tests
    --bench NAME ...             Test only the specified bench target
    --benches                    Test all benches
    --all-targets                Test all targets (default)
    -p SPEC, --package SPEC ...  Package to run tests for
    --all                        Test all packages in the workspace
    --exclude SPEC ...           Exclude packages from the test
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
    -Z FLAG ...                  Unstable (nightly-only) flags to Cargo
```

### `doc-upload`

```
Upload built rustdoc documentation to GitHub pages.

Usage:
    cargo doc-upload [options] [--] [<args>...]

Options:
    -V, --version                Print version info and exit
    --branch NAME ...            Only publish documentation for these branches
                                 Defaults to only the `master` branch
    --token TOKEN                Use the specified GitHub token to publish documentation
                                 If unspecified, checks $GH_TOKEN then attempts to use SSH endpoint
    --message MESSAGE            The message to include in the commit
    --deploy BRANCH              Deploy to the given branch [default: gh-pages]
    --path PATH                  Upload the documentation to the specified remote path [default: /$TRAVIS_BRANCH/]
```

The branch used for doc pushes _may_ be protected, as force-push is not used. Documentation is maintained per-branch
in subdirectories, so `user.github.io/repo/PATH` is where the master branch's documentation lives. `PATH` is by
default the name of the branch, you can overwrite that behavior by passing a custom path into `--path`. A badge is generated
too, like [docs.rs](https://docs.rs/about), that is located at `user.github.io/repo/master/badge.svg`. By default only
master has documentation built, but you can build other branches' docs by passing any number of `--branch NAME`
arguments (the presence of which _will_ disable the default master branch build). Documentation is deployed from
`target/doc`, the default target for `rustdoc`, so make sure to run `cargo doc` before `cargo doc-upload`, and you can
build up whatever directory structure you want in there if you want to document with alternate configurations.

We suggest setting up a `index.html` in the root directory of documentation to redirect to the actual content.
For this purpose we don't touch the root of the `gh-pages` branch (except to create the branch folders) and purposefully
ignore `index.html` in the branch folders. An example `index.html` might look like this:

```html
<meta http-equiv="refresh" content="0; url=my_crate/index.html">
<a href="my_crate/index.html">Redirect</a>
```

This requires Travis to have write-access to your repository. The simplest (and reasonably secure) way to achieve this
is to create a [Personal API Access Token](https://github.com/blog/1509-personal-api-tokens) with `public_repo` scope.
Then on travis, [define the secure environment variable][Travis envvar] `GH_TOKEN` with the value being the new token.

  [Travis envvar]: <https://docs.travis-ci.com/user/environment-variables/#Defining-Variables-in-Repository-Settings>
  [Travis Pro deploy]: <https://blog.travis-ci.com/2012-07-26-travis-pro-update-deploy-keys>
  [Travis encrypt-file]: <https://docs.travis-ci.com/user/encrypting-files/>

This gives any script running on Travis permission to read/write public repositories that you can if they use it
(on non-PR builds only, though keep in mind that bors staging/trying is not a PR build), so be aware of that.
This _does_ work for organization repositories as well, so long as the user's token has permission to write to it.

If you want more security, you can use a [deploy key](https://github.com/blog/2024-read-only-deploy-keys) for
repo-specific access. If you do not provide a token, the script will use SSH to clone from/write to the repository.
[Travis Pro handles the deploy key automatically][Travis Pro deploy], and regular users can use [Travis encrypt-file]
plus a script to move the private key to the correct location.
