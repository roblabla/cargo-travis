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
use std::path::{Path, PathBuf};
use cargo_travis::{CoverageOptions, build_kcov};
use docopt::Docopt;
use cargo_travis::CliError;

pub const USAGE: &'static str = "
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
    flag_quiet: bool,
    flag_color: Option<String>,
    flag_release: bool,
    flag_no_fail_fast: bool,
    flag_frozen: bool,
    flag_locked: bool,
    flag_all: bool,
    flag_exclude: Vec<String>,
    #[serde(rename = "flag_Z")]
    flag_z: Vec<String>,

    // cargo-coveralls flags
    flag_exclude_pattern: Option<String>,
    flag_kcov_build_location: String,
}

impl Options {
    fn into_cargo_args(&self) -> Vec<String> {
        let mut compile_opts = Vec::new();
        for feature in &self.flag_features {
            compile_opts.push(format!("--features={}", feature));
        }
        if self.flag_all_features {
            compile_opts.push("--all-features".into());
        }
        if let Some(s) = &self.flag_jobs {
            compile_opts.push(format!("--jobs={}", s));
        }
        if let Some(s) = &self.flag_manifest_path {
            compile_opts.push(format!("--manifest_path={}", s));
        }
        if self.flag_no_default_features {
            compile_opts.push("--no_default_features".into());
        }
        for s in &self.flag_package {
            compile_opts.push(format!("--package={}", s));
        }
        if let Some(s) = &self.flag_target {
            compile_opts.push(format!("--target={}", s));
        }
        if self.flag_lib {
            compile_opts.push("--lib".into());
        }
        for s in &self.flag_bin {
            compile_opts.push(format!("--bin={}", s));
        }
        if self.flag_bins {
            compile_opts.push("--bins".into());
        }
        for s in &self.flag_test {
            compile_opts.push(format!("--test={}", s));
        }
        if self.flag_tests {
            compile_opts.push("--tests".into());
        }
        for s in &self.flag_bench {
            compile_opts.push(format!("--bench={}", s));
        }
        if self.flag_benches {
            compile_opts.push("--benches".into());
        }
        if self.flag_all_targets {
            compile_opts.push("--all_targets".into());
        }
        if self.flag_verbose != 0 {
            compile_opts.push(format!("-{}", "v".repeat(self.flag_verbose as _)));
        }
        if self.flag_quiet {
            compile_opts.push("--quiet".into());
        }
        if let Some(s) = &self.flag_color {
            compile_opts.push(format!("--color={}", s));
        }
        if self.flag_release {
            compile_opts.push("--release".into());
        }
        if self.flag_no_fail_fast {
            compile_opts.push("--no_fail_fast".into());
        }
        if self.flag_frozen {
            compile_opts.push("--frozen".into());
        }
        if self.flag_locked {
            compile_opts.push("--locked".into());
        }
        if self.flag_all {
            compile_opts.push("--all".into());
        }
        for s in &self.flag_exclude {
            compile_opts.push(format!("--exclude={}", s));
        }
        for s in &self.flag_z {
            compile_opts.push(format!("-Z{}", s));
        }

        compile_opts
    }

}

fn execute(options: Options) -> Result<(), CliError> {
    debug!("executing; cmd=cargo-coveralls; args={:?}",
           env::args().collect::<Vec<_>>());

    if options.flag_version {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let kcov_path = build_kcov(&options.flag_kcov_build_location);
    // TODO: build_kcov() - Might be a good idea to consider linking kcov as a

    let job_id = std::env::var_os("TRAVIS_JOB_ID")
        .expect("Environment variable TRAVIS_JOB_ID not found. This should be run from Travis");

    let ops = CoverageOptions {
        merge_dir: Path::new("target/kcov"),
        merge_args: vec!["--coveralls-id".into(), job_id],
        verbose: options.flag_verbose > 0,
        no_fail_fast: options.flag_no_fail_fast,
        kcov_path: &kcov_path,
        exclude_pattern: options.flag_exclude_pattern.clone(),
        release: options.flag_release,
        manifest_path: options.flag_manifest_path.as_ref().map(PathBuf::from),
        compile_opts: options.into_cargo_args(),
    };

    let err = try!(cargo_travis::run_coverage(&ops, &options.arg_args));

    match err {
        None => Ok(()),
        Some(err) => Err(err),
    }
}

fn main() {
    env_logger::init().unwrap();
    let result = (|| {
        let args: Vec<_> = try!(env::args_os()
            .map(|s| {
                s.into_string().map_err(|s| {
                    format_err!("invalid unicode in argument: {:?}", s)
                })
            })
            .collect());

        let flags = Docopt::new(USAGE)
            .and_then(|d| d.argv(&args[..]).deserialize())
            .unwrap_or_else(|e| e.exit());

        execute(flags).map_err(failure::Error::from)
    })();


    match result {
        Ok(_) => {}
        Err(err) => {
            error!("Error: {}", err);
            match err.downcast::<CliError>() {
                Ok(err) => std::process::exit(err.code()),
                Err(_) => std::process::exit(1),
            }
        }
    }
}
