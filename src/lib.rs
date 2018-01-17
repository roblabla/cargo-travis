extern crate cargo;
extern crate fs_extra;

use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::ffi::{OsString};
use std::fs;
use cargo::core::{Workspace};
use cargo::ops::{CompileOptions};
use cargo::util::{CargoError, CargoErrorKind, CargoTestError, Test};
use cargo::util::process;
use cargo::{CargoResult};

pub struct CoverageOptions<'a> {
    pub compile_opts: CompileOptions<'a>,
    pub merge_dir: &'a Path,
    pub no_fail_fast: bool,
    pub kcov_path: &'a Path,
    pub merge_args: Vec<OsString>, // TODO: Or &[str] ?
    pub exclude_pattern: Option<String>
}

pub fn run_coverage(ws: &Workspace, options: &CoverageOptions, test_args: &[String]) -> CargoResult<Option<CargoTestError>> {
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
            Err(CargoError(CargoErrorKind::ProcessErrorKind(e), .. )) => {
                 errors.push(e);
                if !options.no_fail_fast {
                    return Ok(Some(CargoTestError::new(Test::UnitTest(kind.clone(), test.clone()), errors)))
                }
            }
            Err(e) => {
                //This is an unexpected Cargo error rather than a test failure
                return Err(e)
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
            .arg("master.zip")
            .status()
            .unwrap()
    );

    // Build kcov
    fs::create_dir(&kcov_build_dir);
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

pub fn doc_upload(branch: &str, message: &str, origin: &str, gh_pages: &str) {
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

    let doc_upload_branch = doc_upload.join(branch);
    fs::create_dir(&doc_upload_branch).ok(); // Create dir if not exists
    for entry in doc_upload_branch.read_dir().unwrap() {
        let dir = entry.unwrap();
        // Delete all files in directory, as we'll be copying in everything
        // Ignore index.html (at root) so a redirect page can be manually added
        if dir.file_name() != OsString::from("index.html") {
            let path = dir.path();
            println!("rm -r {}", path.to_string_lossy());
            fs::remove_dir_all(&path).ok();
            fs::remove_file(path).ok();
        }
    }

    let doc = Path::new("target/doc");
    println!("cp {} {}", doc.to_string_lossy(), doc_upload_branch.to_string_lossy());
    let mut last_progress = 0;
    fs_extra::copy_items_with_progress(
        &doc.read_dir().unwrap().map(|entry| entry.unwrap().path()).collect(),
        doc_upload_branch,
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
    // No-op if no changes were made
    require_success(
        Command::new("git")
            .current_dir(doc_upload)
            .arg("commit")
            .arg("--verbose")
            .args(&["-m", message])
            .status()
            .unwrap()
    );

    // Push changes to GitHub
    let status = Command::new("git")
        .current_dir(doc_upload)
        .arg("push")
        .arg(origin)
        .arg(gh_pages)
        .status()
        .unwrap();
    if status.success() {
        println!("Successfully updated documentation.");
    } else {
        println!("Documentation already up-to-date.");
    }
}
