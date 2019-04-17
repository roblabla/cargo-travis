extern crate cargo;
extern crate cargo_travis;
extern crate docopt;
extern crate env_logger;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate log;

use std::env;
use std::path::Path;
use cargo_travis::{CoverageOptions, build_kcov};
use cargo::core::{compiler::BuildConfig, Workspace};
use cargo::util::{Config, CliResult, CliError};
use cargo::ops::{Packages};
use docopt::Docopt;
use failure::err_msg;

pub const USAGE: &'static str = "
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
";


#[derive(Deserialize)]
pub struct Options {
    arg_args: Vec<String>,
    flag_version: bool,
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
    flag_merge_into: String,
    flag_kcov_build_location: String,
}

fn execute(options: Options, config: &mut Config) -> CliResult {
    debug!("executing; cmd=cargo-coverage; args={:?}",
           env::args().collect::<Vec<_>>());

    if options.flag_version {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let kcov_path = build_kcov(options.flag_kcov_build_location);
    // TODO: build_kcov() - Might be a good idea to consider linking kcov as a
    // lib instead ?
    try!(config.configure(options.flag_verbose,
                          options.flag_quiet,
                          &options.flag_color,
                          options.flag_frozen,
                          options.flag_locked,
                          &None,
                          &options.flag_z));

    let ws = if let Some(path) = options.flag_manifest_path {
        try!(Workspace::new(&Path::new(&path), config))
    } else {
        let root = try!(cargo::util::important_paths::find_root_manifest_for_wd(config.cwd()));
        try!(Workspace::new(&root, config))
    };

    let empty = vec![];
    let (mode, filter) = (cargo::core::compiler::CompileMode::Test, cargo::ops::CompileFilter::new(
        options.flag_lib,
        options.flag_bin,
        options.flag_bins,
        options.flag_test,
        options.flag_tests,
        empty,
        false,
        options.flag_bench,
        options.flag_benches,
        options.flag_all_targets
    ));

    let spec = try!(Packages::from_flags(options.flag_all, options.flag_exclude, options.flag_package));

    // TODO: Force compilation target == host, kcov
    let mut build_config = try!(BuildConfig::new(config, options.flag_jobs, &options.flag_target, mode));
    build_config.release = options.flag_release;

    let ops = CoverageOptions {
        merge_dir: Path::new(&options.flag_merge_into),
        merge_args: vec![],
        no_fail_fast: options.flag_no_fail_fast,
        kcov_path: &kcov_path,
        exclude_pattern: options.flag_exclude_pattern,
        compile_opts: cargo::ops::CompileOptions {
            config: config,
            build_config: build_config,
            all_features: options.flag_all_features,
            features: options.flag_features,
            no_default_features: options.flag_no_default_features,
            spec: spec,
            filter: filter,
            target_rustdoc_args: None,
            target_rustc_args: None,
            local_rustdoc_args: None,
            export_dir: None,
        },
    };

    let err = try!(cargo_travis::run_coverage(&ws, &ops, &options.arg_args));

    match err {
        None => Ok(()),
        Some(err) => {
            Err(match err.exit.as_ref().and_then(|e| e.code()) {
                Some(i) => CliError::new(err_msg("test failed"), i),
                None => CliError::new(err.into(), 101)
            })
        }
    }
}

fn main() {
    env_logger::init();
    let mut config = match Config::default() {
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
                    format_err!("invalid unicode in argument: {:?}", s)
                })
            })
            .collect());

        let docopt = Docopt::new(USAGE).unwrap()
            .argv(args.iter().map(|s| &s[..]))
            .help(true);

        let flags = docopt.deserialize().map_err(|e| {
            let code = if e.fatal() {1} else {0};
            CliError::new(e.into(), code)
        })?;

        execute(flags, &mut config)
    })();
    match result {
        Err(e) => cargo::exit_with_error(e, &mut *config.shell()),
        Ok(()) => {}
    }
}
