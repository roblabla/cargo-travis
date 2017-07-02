extern crate cargo;
extern crate cargo_travis;
extern crate env_logger;
extern crate rustc_serialize;

use std::env;
use std::path::Path;
use cargo_travis::{CoverageOptions, build_kcov};
use cargo::core::{Workspace};
use cargo::util::{Config, CliResult, human, Human, CliError};
use cargo::core::shell::{Verbosity, ColorConfig}; 
use cargo::ops::{Packages, MessageFormat};

pub const USAGE: &'static str = "
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
    --all                        Test all packages in the workspace
    -j N, --jobs N               Number of parallel jobs, defaults to # of CPUs
    --release                    Build artifacts in release mode, with optimizations
    --features FEATURES          Space-separated list of features to also build
    --all-features               Build all available features
    --no-default-features        Do not build the `default` feature
    --target TRIPLE              Build for the target triple
    --manifest-path PATH         Path to the manifest to build tests for
    --exclude-pattern PATTERN    Comma-separated  path patterns to exclude from the report
    -v, --verbose ...            Use verbose output
    -q, --quiet                  No output printed to stdout
    --color WHEN                 Coloring: auto, always, never
    --no-fail-fast               Run all tests regardless of failure
    --frozen                     Require Cargo.lock and cache are up to date
    --locked                     Require Cargo.lock is up to date
";


#[derive(RustcDecodable)]
pub struct Options {
    arg_args: Vec<String>,
    flag_all_features: bool,
    flag_merge_into: String,
    flag_features: Vec<String>,
    flag_jobs: Option<u32>,
    flag_manifest_path: Option<String>,
    flag_exclude_pattern: Option<String>,
    flag_no_default_features: bool,
    flag_all: bool,
    flag_package: Vec<String>,
    flag_target: Option<String>,
    flag_lib: bool,
    flag_bin: Vec<String>,
    flag_test: Vec<String>,
    flag_verbose: u32,
    flag_quiet: Option<bool>,
    flag_color: Option<String>,
    flag_release: bool,
    flag_frozen: bool,
    flag_locked: bool,
}

fn execute(options: Options, config: &Config) -> CliResult {
    let kcov_path = build_kcov();
    // lib instead ?
    try!(config.configure(options.flag_verbose,
                          options.flag_quiet,
                          &options.flag_color,
                          options.flag_frozen,
                          options.flag_locked));

    let root = try!(cargo::util::important_paths::find_root_manifest_for_wd(options.flag_manifest_path, config.cwd()));

    let spec = if options.flag_all {
        Packages::All
    } else {
        Packages::Packages(&options.flag_package)
    };

    let empty = vec![];
    let (mode, filter) = (cargo::ops::CompileMode::Test, cargo::ops::CompileFilter::new(
        options.flag_lib,
        &options.flag_bin,
        &options.flag_test,
        &empty,
        &empty));

    // TODO: Shouldn't this be in run_coverage ?
    std::env::set_var("RUSTFLAGS", "-C link-dead-code");
    let ops = CoverageOptions {
        merge_dir: Path::new(&options.flag_merge_into),
        merge_args: vec![],
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

    let ws = try!(Workspace::new(&root, config));

    let err = try!(cargo_travis::run_coverage(&ws, &ops, &options.arg_args));

    match err {
        None => Ok(()),
        Some(err) => {
            Err(match err.exit.as_ref().and_then(|e| e.code()) {
                Some(i) => CliError::new(human("test failed"), i),
                None => CliError::new(Box::new(Human(err)), 101)
            })
        }
    }
}

fn main() {
    env_logger::init().unwrap();
    let config = match Config::default() {
        Ok(cfg) => cfg,
        Err(e) => {
             let mut shell = cargo::shell(Verbosity::Verbose, ColorConfig::Auto);
             cargo::exit_with_error(e.into(), &mut shell)
        }
    };
    let result = (|| {
        let args: Vec<_> = try!(env::args_os()
            .map(|s| {
                s.into_string().map_err(|s| {
                    human(format!("invalid unicode in argument: {:?}", s))
                })
            })
            .collect());
        let rest = &args;
        config.shell().set_verbosity(Verbosity::Verbose);
        cargo::call_main_without_stdin(execute, &config, USAGE, rest, false)
    })();
    match result {
        Err(e) => cargo::exit_with_error(e, &mut *config.shell()),
        Ok(()) => {}
    }
}
