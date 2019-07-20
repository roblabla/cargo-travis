extern crate badge;
extern crate cargo_metadata;
extern crate fs_extra;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate log;
#[macro_use]
extern crate failure_derive;
extern crate failure;

mod process;

use badge::{Badge, BadgeOptions};
use cargo_metadata::{MetadataCommand, PackageId, Message, Artifact, ArtifactProfile};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Stdio, ExitStatus, Output, Command};
use std::str;

use process::Process;

#[derive(Fail, Debug)]
pub enum CliError {
    #[fail(display = "{}", desc)]
    TestError {
        desc: String,
        test: Option<(PackageId, String)>,
        errors: Vec<CliError>,
    },
    #[fail(display = "Failed to spawn {:?}", name)]
    SpawnError {
        name: OsString,
        args: Vec<OsString>,
        #[cause] cause: std::io::Error,
    },
    #[fail(display = "{}", desc)]
    ProcessError {
        desc: String,
        exit: ExitStatus,
    },
    #[fail(display = "Failed to find crate root")]
    CargoMetadataError {
        #[cause] cause: cargo_metadata::Error
    },
    #[fail(display = "{}", desc)]
    OtherError {
        desc: String,
        code: i32,
    }
}

impl From<cargo_metadata::Error> for CliError {
    fn from(error: cargo_metadata::Error) -> CliError {
        CliError::CargoMetadataError {
            cause: error
        }
    }
}

impl CliError {
    pub fn code(&self) -> i32 {
        match self {
            CliError::ProcessError { exit, .. } => exit.code().unwrap_or(1),
            CliError::OtherError { code, .. } => *code,
            _ => 1
        }
    }

    fn new_test_error(test: Option<(PackageId, String)>, errors: Vec<CliError>) -> CliError {
        if errors.is_empty() {
            panic!("Cannot create CargoTestError from empty Vec")
        }
        let desc = errors
            .iter()
            .map(|error| error.to_string())
            .collect::<Vec<String>>()
            .join("\n");

        CliError::TestError {
            desc: desc,
            test,
            errors,
        }
    }

    fn process_error(msg: &str, output: Option<Output>, status: ExitStatus) -> CliError {
        let exit = match status.code() {
            Some(_s) => status.to_string(),
            None => "Unknown error".to_string(),
        };
        let mut desc = format!("{} ({})", &msg, exit);
        if let Some(output) = output {
            match str::from_utf8(&output.stdout) {
                Ok(s) if !s.trim().is_empty() => {
                    desc.push_str("\n--- stdout\n");
                    desc.push_str(s);
                }
                Ok(..) | Err(..) => {}
            }
            match str::from_utf8(&output.stderr) {
                Ok(s) if !s.trim().is_empty() => {
                    desc.push_str("\n--- stderr\n");
                    desc.push_str(s);
                }
                Ok(..) | Err(..) => {}
            }
        }
        CliError::ProcessError {
            desc,
            exit: status,
        }
    }
}

pub struct CoverageOptions<'a> {
    pub compile_opts: Vec<String>,
    pub release: bool,
    pub verbose: bool,
    pub manifest_path: Option<PathBuf>,
    pub merge_dir: &'a Path,
    pub no_fail_fast: bool,
    pub kcov_path: &'a Path,
    pub merge_args: Vec<OsString>,
    pub exclude_pattern: Option<String>
}

pub fn run_coverage(options: &CoverageOptions, test_args: &[String]) -> Result<Option<CliError>, CliError> {
    // TODO: It'd be nice if there was a flag in compile_opts for this.
    // The compiler needs to be told to not remove any code that isn't called or
    // it'll be missed in the coverage counts, but the existing user-provided
    // RUSTFLAGS should be preserved as well (and should be put last, so that
    // they override any earlier repeats).
    let mut rustflags: std::ffi::OsString = "-C link-dead-code".into();
    if options.release {
        // In release mode, ensure that there's debuginfo in some form so that
        // kcov has something to work with.
        rustflags.push(" -C debuginfo=2");
    }

    // Acquire metadata for the project, used later to get the target directory.
    let mut metadata = MetadataCommand::new();
    if let Some(ref s) = options.manifest_path {
        metadata.manifest_path(s);
    }
    let metadata = metadata.exec()?;

    if let Some(existing) = std::env::var_os("RUSTFLAGS") {
        rustflags.push(" ");
        rustflags.push(existing);
    }
    std::env::set_var("RUSTFLAGS", rustflags);

    let mut compilation = Process::new("cargo");
    compilation.arg("test");
    compilation.arg("--no-run");
    compilation.arg("--message-format=json");
    compilation.args(&options.compile_opts);
    compilation.stdout(Stdio::piped());
    // Prevent the "building" bar from showing up.
    compilation.stderr(Stdio::null());
    let mut compilation = compilation.spawn()?;

    let mut tests = cargo_metadata::parse_messages(compilation.stdout.take().unwrap())
        .filter_map(|item| {
            debug!("{:?}", item);
            match item {
                Ok(Message::CompilerMessage(msg)) => { let _ = std::io::stdout().write_fmt(format_args!("{}", msg)); None },
                Ok(Message::CompilerArtifact(item @ Artifact { executable: Some(_), profile: ArtifactProfile { test: true, .. }, .. }))
                    => {
                    if options.verbose {
                        println!("Compiled {}", item.package_id);
                    } else {
                        println!("Compiled {}", item.target.name);
                    }
                    Some(item)
                },
                Ok(_) => None,
                Err(err) => { error!("Failed to parse cargo message: {}\nThis is an error in cargo-travis. Please report it!", err); None }
            }
        })
        .collect::<Vec<_>>();

    compilation.wait_success()?;

    if tests.is_empty() {
        return Err(CliError::OtherError {
            desc: "No tests found.".into(),
            code: 1
        })
    }

    compilation.wait().expect("Couldn't get cargo's exit status.");

    tests.sort_by(|a, b| {
        (&a.package_id, &a.target.name).cmp(&(&b.package_id, &b.target.name))
    });

    let cwd = std::env::current_dir().unwrap();
    let mut errors = vec![];

    let v : Vec<std::ffi::OsString> = test_args.iter().cloned().map::<std::ffi::OsString, _>(|val| val.into()).collect();

    for Artifact { package_id: pkg, target, executable: exe, .. } in &tests {
        let exe = exe.as_ref().expect("Previous filter_map should have only returned elements containing an executable.");
        let to_display = match exe.strip_prefix(&cwd) {
            Ok(path) => path,
            Err(_) => &*exe
        };

        // DLYB trick on OSX is here v
        // TODO: Run tests (target processes) with the cargo runner.
        //let mut cmd = try!(compilation.target_process(options.kcov_path, pkg));
        let mut cmd = Process::new(options.kcov_path);
        // TODO: Make all that more configurable
        let mut kcov_target_path = OsString::from("kcov-".to_string());
        kcov_target_path.push(to_display.file_name().unwrap());
        let target_dir = metadata.target_directory.join(kcov_target_path);
        let default_include_path = format!("--include-path={}", metadata.workspace_root.display());

        let mut args = vec![
            OsString::from("--verify"),
            OsString::from(default_include_path),
            OsString::from(target_dir)];

        // add exclude path
        if let Some(ref exclude) = options.exclude_pattern {
            let exclude_option = OsString::from(format!("--exclude-pattern={}", exclude));
            args.push(exclude_option);
        }

        args.push(OsString::from(exe));

        args.extend(v.clone());
        cmd.args(&args);
        if !options.verbose {
            println!("Running {}", to_display.display());
        } else {
            println!("Running {}", cmd);
        }

        let result = cmd.exec();

        match result {
            Err(e) => {
                errors.push(e);
                if !options.no_fail_fast {
                    return Ok(Some(CliError::new_test_error(Some((pkg.clone(), target.name.clone())), errors)))
                }
            }
            Ok(_) => {}
        }
    }

    // Let the user pass mergeargs
    let mut mergeargs : Vec<OsString> = vec!["--merge".to_string().into(), options.merge_dir.as_os_str().to_os_string()];
    mergeargs.extend(options.merge_args.iter().cloned());
    mergeargs.extend(tests.iter().map(|&Artifact { executable: ref exe, .. }| {
        let mut kcov_final_path = OsString::from("kcov-".to_string());
        kcov_final_path.push(exe.as_ref().unwrap().file_name().unwrap());
        metadata.target_directory.join(kcov_final_path).into()
    }));
    let mut cmd = Process::new(options.kcov_path.as_os_str().to_os_string());
    cmd.args(&mergeargs);

    if !options.verbose {
        println!("Merging coverage {}", options.merge_dir.display());
    } else {
        println!("Merging coverage {}", cmd);
    }
    try!(cmd.status());
    if errors.is_empty() {
        Ok(None)
    } else {
        Ok(Some(CliError::new_test_error(None, errors)))
    }
}

fn require_success(status: std::process::ExitStatus) {
    if !status.success() {
        std::process::exit(status.code().unwrap())
    }
}

pub fn build_kcov<P: AsRef<Path>>(kcov_dir: P) -> PathBuf {
    // If kcov is in path
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            if path.join("kcov").exists() {
                return path.join("kcov");
            }
        }
    }

    let kcov_dir: &Path = kcov_dir.as_ref();
    let kcov_master_dir = kcov_dir.join("kcov-master");
    let kcov_build_dir = kcov_master_dir.join("build");
    let kcov_built_path = kcov_build_dir.join("src/kcov");

    // If we already built kcov
    if kcov_built_path.exists() {
        return kcov_built_path;
    }

    // Download kcov
    println!("Downloading kcov");
    require_success(
        Process::new("wget")
            .current_dir(kcov_dir)
            .arg("https://github.com/SimonKagstrom/kcov/archive/master.zip")
            .status()
            .unwrap()
    );

    // Extract kcov
    println!("Extracting kcov");
    require_success(
        Process::new("unzip")
            .current_dir(kcov_dir)
            .arg("master.zip")
            .status()
            .unwrap()
    );

    // Build kcov
    fs::create_dir(&kcov_build_dir).expect(&format!("Failed to created dir {:?} for kcov", kcov_build_dir));
    println!("CMaking kcov");
    require_success(
        Process::new("cmake")
            .current_dir(&kcov_build_dir)
            .arg("..")
            .status()
            .unwrap()
    );
    println!("Making kcov");
    require_success(
        Process::new("make")
            .current_dir(&kcov_build_dir)
            .status()
            .unwrap()
    );

    assert!(kcov_build_dir.exists());
    kcov_built_path
}

pub fn doc_upload(message: &str, origin: &str, gh_pages: &str, doc_path: &str, local_doc_path: &Path, clobber_index: bool) -> Result<(), CliError> {
    let doc_upload = Path::new("target/doc-upload");

    if !doc_upload.exists() {
        // If the folder doesn't exist, clone it from remote
        // ASSUME: if target/doc-upload exists, it's ours
        let status = Process::new("git")
            .arg("clone")
            .arg("--verbose")
            .args(&["--branch", gh_pages])
            .args(&["--depth", "1"])
            .arg(origin)
            .arg(doc_upload)
            .status()
            .unwrap();
        if !status.success() {
            // If clone fails, that means that the remote doesn't exist
            // So we create a new repository for the documentation branch
            require_success(
                Process::new("git")
                    .arg("init")
                    .arg(doc_upload)
                    .status()
                    .unwrap()
            );
            require_success(
                Process::new("git")
                    .current_dir(doc_upload)
                    .arg("checkout")
                    .args(&["-b", gh_pages])
                    .status()
                    .unwrap()
            );
        }
    }

    let doc_upload_branch = doc_upload.join(doc_path);

    println!("mkdir {}", doc_upload_branch.display());
    let res = fs::create_dir(&doc_upload_branch);

    match res.as_ref().map_err(|err| err.kind()) {
        Err(std::io::ErrorKind::AlreadyExists) | Ok(()) => (),
        Err(_) => panic!("{:?}", res),
    }

    // we can't canonicalize before we create the folder
    let doc_upload_branch = doc_upload_branch.canonicalize().unwrap();

    if !doc_upload_branch.starts_with(env::current_dir().unwrap().join(doc_upload)) {
        return Err(CliError::OtherError {
            desc: "Path passed in `--path` is outside the intended `target/doc-upload` folder".to_string(),
            code: 1
        });
    }

    for entry in doc_upload_branch.read_dir().unwrap() {
        let dir = entry.unwrap();
        // Delete all files in directory, as we'll be copying in everything
        // Ignore index.html (at root) so a redirect page can be manually added
        // Unless user wants otherwise (--clobber-index)
        // Or a new one was generated
        if dir.file_name() != OsString::from("index.html")
            || clobber_index
            || local_doc_path.join("index.html").exists()
        {
            let path = dir.path();
            println!("rm -r {}", path.to_string_lossy());
            fs::remove_dir_all(&path).ok();
            fs::remove_file(path).ok();
        }
    }

    // default badge shows that no successful build was made
    let mut badge_status = "no builds".to_string();
    let mut badge_color = "#e05d44".to_string();

    // try to read manifest to extract version number
    
    let version = MetadataCommand::new().exec()
        .map_err(|err| error!("Couldn't generate workspace: {}", err))
        .ok()
        .and_then(|metadata| if metadata.workspace_members.len() == 1 {
            let member = metadata.workspace_members[0].clone();
            Some((metadata, member))
        } else {
            error!("Couldn't get package: workspaces not supported");
            None
        })
        .and_then(|(metadata, v)| metadata.packages.into_iter().find(|pkg| pkg.id == v))
        .map(|pkg| format!("{}", pkg.version));

    let mut manifest = env::current_dir().unwrap();
    manifest.push("Cargo.toml");

    // update badge to contain version number
    if let Some(version) = &version {
        badge_status = version.clone();
    }

    let doc = local_doc_path;
    println!("cp {} {}", doc.to_string_lossy(), doc_upload_branch.to_string_lossy());
    let mut last_progress = 0;

    let mut result = Ok(());

    if let Ok(doc) = doc.read_dir() {
        fs_extra::copy_items_with_progress(
            &doc.map(|entry| entry.unwrap().path()).collect(),
            &doc_upload_branch,
            &fs_extra::dir::CopyOptions::new(),
            |info| {
                // Some documentation can be very large, especially with a large number of dependencies
                // Don't go silent during copy, give updates every MiB processed
                if info.copied_bytes >> 20 > last_progress {
                    last_progress = info.copied_bytes >> 20;
                    println!("{}/{} MiB", info.copied_bytes >> 20, info.total_bytes >> 20);
                }
                fs_extra::dir::TransitProcessResult::ContinueOrAbort
            }
        ).unwrap();

        // update the badge to reflect build was successful
        // but only if we managed to extract a version number
        if version.is_some() {
            badge_color = "#4d76ae".to_string();
        }
    }
    else {
        println!("No documentation found to upload.");
        result = Err(CliError::OtherError { desc: "No documentation generated".to_string(), code: 1 });
    }
    
    // make badge.json
    let json = json!({
        "schemaVersion": 1,
        "label": "docs",
        "message": badge_status,
        "color": badge_color
    });

    let mut file = fs::File::create(doc_upload_branch.join("badge.json")).unwrap();
    file.write_all(json.to_string().as_bytes()).unwrap();

    // make badge.svg
    let badge_options = BadgeOptions {
        subject: "docs".to_string(),
        status: badge_status.to_string(),
        color: badge_color.to_string(),
    };

    let mut file = fs::File::create(doc_upload_branch.join("badge.svg")).unwrap();
    file.write_all(Badge::new(badge_options).unwrap().to_svg().as_bytes()).unwrap();

    // Tell git to track all of the files we copied over
    // Also tracks deletions of files if things changed
    require_success(
        Process::new("git")
            .current_dir(doc_upload)
            .arg("add")
            .arg("--verbose")
            .arg("--all")
            .status()
            .unwrap()
    );

    // Save the changes
    if Process::new("git")
        .current_dir(doc_upload)
        .arg("commit")
        .arg("--verbose")
        .args(&["-m", message])
        .status().is_err()
    {
        println!("No changes to the documentation.");
    } else {
        // Push changes to GitHub
        require_success(
            Command::new("git")
                .current_dir(doc_upload)
                .arg("push")
                .arg(origin)
                .arg(gh_pages)
                .status()
                .unwrap(),
        );
    }
    result
}