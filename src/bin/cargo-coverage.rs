extern crate cargo;
extern crate cargo_travis;
extern crate env_logger;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate log;

use std::env;
use std::path::Path;
use cargo_travis::{CoverageOptions, build_kcov};
use cargo::core::{Workspace};
use cargo::util::{CargoError, CargoErrorKind, Config, CliResult, CliError};
use cargo::ops::{Packages, MessageFormat};

pub const USAGE: &'static str = "
Record coverage of `cargo test`, this runs all binaries that `cargo test` runs
but not doc tests. The results of all tests are merged into a single directory

Usage:
    cargo coverage [options] [--] [<args>...]

Coverage Options:
    -m PATH, --merge-into PATH   Path to the directory to put the final merged
                                 kcov result into [default: target/kcov]
    --exclude-pattern PATTERN    Comma-separated path patterns to exclude from the report

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
";


#[derive(Deserialize)]
pub struct Options {
    arg_args: Vec<String>,
    flag_features: Vec<String>,
    flag_all_features: bool,
    flag_jobs: Option<u32>,
    flag_manifest_path: Option<String>,
    flag_no_default_features: bool,
    flag_package: Vec<String>,
    flag_target: Option<String>,
    flag_lib: bool,
    flag_bin: Vec<String>,
    flag_bins: bool,
    flag_test: Vec<String>,
    flag_tests: bool,
    flag_bench: Vec<String>,
    flag_benches: bool,
    flag_all_targets: bool,
    flag_verbose: u32,
    flag_quiet: Option<bool>,
    flag_color: Option<String>,
    flag_release: bool,
    flag_no_fail_fast: bool,
    flag_frozen: bool,
    flag_locked: bool,
    flag_all: bool,
    flag_exclude: Vec<String>,
    #[serde(rename = "flag_Z")]
    flag_z: Vec<String>,

    // cargo-coverage flags
    flag_exclude_pattern: Option<String>,
    flag_merge_into: String
}

fn execute(options: Options, config: &Config) -> CliResult {
    debug!("executing; cmd=cargo-coverage; args={:?}",
           env::args().collect::<Vec<_>>());

    let kcov_path = build_kcov();
    // TODO: build_kcov() - Might be a good idea to consider linking kcov as a
    // lib instead ?
    try!(config.configure(options.flag_verbose,
                          options.flag_quiet,
                          &options.flag_color,
                          options.flag_frozen,
                          options.flag_locked,
                          &options.flag_z));

    let root = try!(cargo::util::important_paths::find_root_manifest_for_wd(options.flag_manifest_path, config.cwd()));
    let ws = try!(Workspace::new(&root, config));

    let empty = vec![];
    let (mode, filter) = (cargo::ops::CompileMode::Test, cargo::ops::CompileFilter::new(
        options.flag_lib,
        &options.flag_bin,
        options.flag_bins,
        &options.flag_test,
        options.flag_tests,
        &empty,
        false,
        &options.flag_bench,
        options.flag_benches,
        options.flag_all_targets
    ));

    let spec = try!(Packages::from_flags(ws.is_virtual(), options.flag_all, &options.flag_exclude, &options.flag_package));

    // TODO: Shouldn't this be in run_coverage ?
    // TODO: It'd be nice if there was a flag in compile_opts for this.
    std::env::set_var("RUSTFLAGS", "-C link-dead-code");

    let ops = CoverageOptions {
        merge_dir: Path::new(&options.flag_merge_into),
        merge_args: vec![],
        no_fail_fast: options.flag_no_fail_fast,
        kcov_path: &kcov_path,
        exclude_pattern: options.flag_exclude_pattern,
        compile_opts: cargo::ops::CompileOptions {
            config: config,
            jobs: options.flag_jobs,
            target: options.flag_target.as_ref().map(|s| &s[..]), // TODO: Force compilation target == host, kcov
            message_format: MessageFormat::Human, // TODO: Allow to change this
            all_features: options.flag_all_features,
            features: &options.flag_features,
            no_default_features: options.flag_no_default_features,
            spec: spec,
            release: options.flag_release,
            mode: mode,
            filter: filter,
            target_rustdoc_args: None,
            target_rustc_args: None
        },
    };

    let err = try!(cargo_travis::run_coverage(&ws, &ops, &options.arg_args));

    match err {
        None => Ok(()),
        Some(err) => {
            Err(match err.exit.as_ref().and_then(|e| e.code()) {
                Some(i) => CliError::new("test failed".into(), i),
                None => CliError::new(CargoErrorKind::CargoTestErrorKind(err).into(), 101)
            })
        }
    }
}

fn main() {
    env_logger::init().unwrap();
    let config = match Config::default() {
        Ok(cfg) => cfg,
        Err(e) => {
             let mut shell = cargo::core::Shell::new();
             cargo::exit_with_error(e.into(), &mut shell)
        }
    };
    let result = (|| {
        let args: Vec<_> = try!(env::args_os()
            .map(|s| {
                s.into_string().map_err(|s| {
                    CargoError::from(format!("invalid unicode in argument: {:?}", s))
                })
            })
            .collect());
        let rest = &args;
        cargo::call_main_without_stdin(execute, &config, USAGE, rest, false)
    })();
    match result {
        Err(e) => cargo::exit_with_error(e, &mut *config.shell()),
        Ok(()) => {}
    }
}
