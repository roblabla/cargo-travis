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
use docopt::Docopt;
use cargo_travis::CliError;

// Note about --path: we don't use the proper default syntax because the default
// value depends on an env variable.
pub const USAGE: &'static str = "
Upload built rustdoc documentation to GitHub pages.

Usage:
    cargo doc-upload [options]

Options:
    -h, --help                   Print this message
    -V, --version                Print version info and exit
    --branch NAME ...            Only publish documentation for these branches
                                 Defaults to only the `master` branch
    --token TOKEN                Use the specified GitHub token to publish documentation
                                 If unspecified, checks $GH_TOKEN then attempts to use SSH endpoint
    --message MESSAGE            The message to include in the commit
    --deploy BRANCH              Deploy to the given branch [default: gh-pages]
    --path PATH                  Upload the documentation to the specified remote path (defaults to $TRAVIS_BRANCH/)
    --clobber-index              Delete `index.html` from repo
    --target TRIPLE              Fetch the documentation for the target triple
";

#[derive(Deserialize)]
pub struct Options {
    flag_version: bool,
    flag_branch: Vec<String>,
    flag_token: Option<String>,
    flag_message: Option<String>,
    flag_deploy: Option<String>,
    flag_path: Option<String>,
    flag_clobber_index: bool,
    flag_target: Option<String>,
}

fn execute(options: Options) -> Result<(), CliError> {
    debug!("executing; cmd=cargo-doc-upload; env={:?}",
           env::args().collect::<Vec<_>>());

    if options.flag_version {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let branches = if options.flag_branch.is_empty() {
        vec!["master".to_string()]
    } else {
        options.flag_branch
    };

    let branch = env::var("TRAVIS_BRANCH").expect("$TRAVIS_BRANCH not set");
    if !branches.contains(&branch) {
        println!("Skipping branch {}", branch);
        return Ok(());
    }

    let pull_request = env::var("TRAVIS_PULL_REQUEST").expect("$TRAVIS_PULL_REQUEST not set");
    if pull_request != "false" {
        println!("Skipping PR");
        return Ok(());
    }

    let path = options.flag_path.unwrap_or_else(|| branch.clone());

    // TODO FEAT: Allow passing origin string
    let token = options.flag_token.or(env::var("GH_TOKEN").ok());
    let slug = env::var("TRAVIS_REPO_SLUG").expect("$TRAVIS_REPO_SLUG not set");
    let origin = if let Some(token) = token {
        format!("https://{}@github.com/{}.git", token, slug)
    } else {
        eprintln!("GitHub Personal Access Token was not provided in $GH_TOKEN or --token");
        eprintln!("Falling back to using the SSH endpoint");
        format!("git@github.com:{}.git", slug)
    };

    let message = options.flag_message.unwrap_or("Automatic Travis documentation build".to_string());
    let gh_pages = options.flag_deploy.unwrap_or("gh-pages".to_string());
    let clobber_index = options.flag_clobber_index;

    let local_doc_path = options.flag_target
        .map(|v| Path::new("target").join(v).join("doc"))
        .unwrap_or(PathBuf::from("target/doc"));

    cargo_travis::doc_upload(&message, &origin, &gh_pages, &path, &local_doc_path, clobber_index)
}

fn main() {
    env_logger::init().unwrap();
    let result: Result<(), failure::Error> = (|| {
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

        execute(flags).map_err(|err| err.into())
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
