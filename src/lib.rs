extern crate badge;
extern crate cargo;
extern crate fs_extra;
#[macro_use]
extern crate serde_json;

use badge::{Badge, BadgeOptions};
use cargo::core::Workspace;
use cargo::ops::CompileOptions;
use cargo::util::{config::Config, errors::ProcessError, process, CargoTestError, Test};
use cargo::CargoResult;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

pub struct CoverageOptions<'a> {
    pub compile_opts: CompileOptions<'a>,
    pub merge_dir: &'a Path,
    pub no_fail_fast: bool,
    pub kcov_path: &'a Path,
    pub merge_args: Vec<OsString>, // TODO: Or &[str] ?
    pub exclude_pattern: Option<String>
}

pub fn run_coverage(ws: &Workspace, options: &CoverageOptions, test_args: &[String]) -> CargoResult<Option<CargoTestError>> {
    // TODO: It'd be nice if there was a flag in compile_opts for this.

    // The compiler needs to be told to not remove any code that isn't called or
    // it'll be missed in the coverage counts, but the existing user-provided
    // RUSTFLAGS should be preserved as well (and should be put last, so that
    // they override any earlier repeats).
    let mut rustflags: std::ffi::OsString = "-C link-dead-code".into();
    if options.compile_opts.build_config.release {
        // In release mode, ensure that there's debuginfo in some form so that
        // kcov has something to work with.
        rustflags.push(" -C debuginfo=2");
    }
    if let Some(existing) = std::env::var_os("RUSTFLAGS") {
        rustflags.push(" ");
        rustflags.push(existing);
    }
    std::env::set_var("RUSTFLAGS", rustflags);


    let mut compilation = try!(cargo::ops::compile(ws, &options.compile_opts));
    compilation.tests.sort_by(|a, b| {
        (a.0.package_id(), &a.1).cmp(&(b.0.package_id(), &b.1))
    });

    let config = options.compile_opts.config;
    let cwd = options.compile_opts.config.cwd();

    let mut errors = vec![];

    let v : Vec<std::ffi::OsString> = test_args.iter().cloned().map::<std::ffi::OsString, _>(|val| val.into()).collect();

    //let x = &compilation.tests.map(run_single_coverage);

    for &(ref pkg, ref kind, ref test, ref exe) in &compilation.tests {
        let to_display = match cargo::util::without_prefix(exe, &cwd) {
            Some(path) => path,
            None => &**exe
        };

        // DLYB trick on OSX is here v
        let mut cmd = try!(compilation.target_process(options.kcov_path, pkg));
        // TODO: Make all that more configurable
        //TODO: The unwraps shouldn't cause problems... right ?
        let target = ws.target_dir().join("kcov-".to_string() + to_display.file_name().unwrap().to_str().unwrap()).into_path_unlocked();
        let default_include_path = format!("--include-path={}", ws.root().display());

        let mut args = vec![
            OsString::from("--verify"),
            OsString::from(default_include_path),
            OsString::from(target)];

        // add exclude path
        if let Some(ref exclude) = options.exclude_pattern {
            let exclude_option = OsString::from(format!("--exclude-pattern={}", exclude));
            args.push(exclude_option);
        }

        args.push(OsString::from(exe));

        args.extend(v.clone());
        cmd.args(&args);
        try!(config.shell().concise(|shell| {
            shell.status("Running", to_display.display().to_string())
        }));
        try!(config.shell().verbose(|shell| {
            shell.status("Running", cmd.to_string())
        }));

        let result = cmd.exec();

        match result {
            Err(e) => {
                match e.downcast::<ProcessError>() {
                    Ok(e) => {
                        errors.push(e);
                        if !options.no_fail_fast {
                            return Ok(Some(CargoTestError::new(Test::UnitTest {
                                kind: kind.clone(),
                                name: test.clone(),
                                pkg_name: pkg.name().to_string(),
                            }, errors)))
                        }
                    }
                    Err(e) => {
                        //This is an unexpected Cargo error rather than a test failure
                        return Err(e)
                    }
                }
            }
            Ok(()) => {}
        }
    }

    // Let the user pass mergeargs
    let mut mergeargs : Vec<OsString> = vec!["--merge".to_string().into(), options.merge_dir.as_os_str().to_os_string()];
    mergeargs.extend(options.merge_args.iter().cloned());
    mergeargs.extend(compilation.tests.iter().map(|&(_, _, _, ref exe)|
        ws.target_dir().join("kcov-".to_string() + exe.file_name().unwrap().to_str().unwrap()).into_path_unlocked().into()
    ));
    let mut cmd = process(options.kcov_path.as_os_str().to_os_string());
    cmd.args(&mergeargs);
    try!(config.shell().concise(|shell| {
        shell.status("Merging coverage", options.merge_dir.display().to_string())
    }));
    try!(config.shell().verbose(|shell| {
        shell.status("Merging coverage", cmd.to_string())
    }));
    try!(cmd.exec());
    if errors.is_empty() {
        Ok(None)
    } else {
        Ok(Some(CargoTestError::new(Test::Multiple, errors)))
    }
}

fn require_success(status: process::ExitStatus) {
    if !status.success() {
        process::exit(status.code().unwrap())
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
        Command::new("wget")
            .current_dir(kcov_dir)
            .arg("https://github.com/SimonKagstrom/kcov/archive/master.zip")
            .status()
            .unwrap()
    );

    // Extract kcov
    println!("Extracting kcov");
    require_success(
        Command::new("unzip")
            .current_dir(kcov_dir)
            .arg("-o")
            .arg("master.zip")
            .status()
            .unwrap()
    );

    // Build kcov
    fs::create_dir(&kcov_build_dir).expect(&format!("Failed to created dir {:?} for kcov", kcov_build_dir));
    println!("CMaking kcov");
    require_success(
        Command::new("cmake")
            .current_dir(&kcov_build_dir)
            .arg("..")
            .status()
            .unwrap()
    );
    println!("Making kcov");
    require_success(
        Command::new("make")
            .current_dir(&kcov_build_dir)
            .status()
            .unwrap()
    );

    assert!(kcov_build_dir.exists());
    kcov_built_path
}

pub fn doc_upload(message: &str, origin: &str, gh_pages: &str, doc_path: &str, local_doc_path: &Path, clobber_index: bool) -> Result<(), (String, i32)> {
    let doc_upload = Path::new("target/doc-upload");

    if !doc_upload.exists() {
        // If the folder doesn't exist, clone it from remote
        // ASSUME: if target/doc-upload exists, it's ours
        let status = Command::new("git")
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
                Command::new("git")
                    .arg("init")
                    .arg(doc_upload)
                    .status()
                    .unwrap()
            );
            require_success(
                Command::new("git")
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
        return Err(("Path passed in `--path` is outside the intended `target/doc-upload` folder".to_string(), 1));
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
    let config = Config::default().expect("failed to create cargo Config");
    let mut version = Err(());

    let mut manifest = env::current_dir().unwrap();
    manifest.push("Cargo.toml");

    match Workspace::new(&manifest, &config) {
        Ok(workspace) => match workspace.current() {
            Ok(package) => version = Ok(format!("{}", package.manifest().version())),
            Err(error) => println!("couldn't get package: {}", error),
        },
        Err(error) => println!("couldn't generate workspace: {}", error),
    }

    // update badge to contain version number
    if let Ok(version) = &version {
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
        if version.is_ok() {
            badge_color = "#4d76ae".to_string();
        }
    }
    else {
        println!("No documentation found to upload.");
        result = Err(("No documentation generated".to_string(), 1));
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
        Command::new("git")
            .current_dir(doc_upload)
            .arg("add")
            .arg("--verbose")
            .arg("--all")
            .status()
            .unwrap()
    );

    // Save the changes
    if Command::new("git")
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
