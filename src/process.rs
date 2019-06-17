//! Process utilities
//!
//! Wrapper around [std::process::Command] with improved error handling and
//! utilities to run non-host-native commands.

// TODO: Implement target_runner.

use std::process::{Command, Child as ProcessChild, ExitStatus, Output};
use std::ffi::{OsStr, OsString};
use std::ops::{Deref, DerefMut};
use std::fmt;
use CliError;

pub struct Process {
    name: OsString,
    args: Vec<OsString>,
    cmd: Command
}

impl fmt::Display for Process {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as fmt::Debug>::fmt(self, f)
    }
}

impl fmt::Debug for Process {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.name)?;
        for arg in &self.args {
            write!(f, " {:?}", arg)?;
        }
        Ok(())
    }
}

impl Deref for Process {
    type Target = Command;

    fn deref(&self) -> &Command {
        &self.cmd
    }
}

impl DerefMut for Process {
    fn deref_mut(&mut self) -> &mut Command {
        &mut self.cmd
    }
}

impl Process {
    pub fn new<T: AsRef<OsStr>>(program: T) -> Process {
        Process {
            name: program.as_ref().into(),
            args: Vec::new(),
            cmd: Command::new(program)
        }
    }

    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Process {
        self.args.push(arg.as_ref().into());
        self.cmd.arg(arg);
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Process
    where
        I: IntoIterator<IntoIter = S, Item = S::Item>,
        S: Iterator,
        S: Clone,
        S::Item: AsRef<OsStr>,
    {
        let iter = args.into_iter();
        self.args.extend(iter.clone().map(|v| v.as_ref().into()));
        self.cmd.args(iter);
        self
    }

    pub fn spawn(&mut self) -> Result<Child, CliError> {
        self.cmd.spawn()
            .map_err(|err| CliError::SpawnError {
                name: self.name.clone(),
                args: self.args.clone(),
                cause: err
            })
            .map(|v| Child {
                child: v,
                name: self.name.clone(),
                args: self.args.clone()
            })
    }

    pub fn status(&mut self) -> Result<ExitStatus, CliError> {
        self.cmd.status()
            .map_err(|err| CliError::SpawnError {
                name: self.name.clone(),
                args: self.args.clone(),
                cause: err
            })
    }

    #[allow(dead_code)]
    pub fn output(&mut self) -> Result<Output, CliError> {
        self.cmd.output()
            .map_err(|err| CliError::SpawnError {
                name: self.name.clone(),
                args: self.args.clone(),
                cause: err
            })
    }

    pub fn exec(&mut self) -> Result<(), CliError> {
        self.status()
            .and_then(|status| if status.success() {
                Ok(())
            } else {
                Err(CliError::process_error(&format!("process didn't exit successfully: {}", self), None, status))
            })
    }

    #[allow(dead_code)]
    pub fn exec_with_output(&mut self) -> Result<Output, CliError> {
        self.output()
            .and_then(|output| if output.status.success() {
                Ok(output)
            } else {
                let status = output.status;
                Err(CliError::process_error(&format!("process didn't exit successfully: {}", self), Some(output), status))
            })
    }
}

pub struct Child {
    name: OsString,
    args: Vec<OsString>,
    child: ProcessChild
}

impl fmt::Debug for Child {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.name)?;
        for arg in &self.args {
            write!(f, " {:?}", arg)?;
        }
        Ok(())
    }
}

impl Deref for Child {
    type Target = ProcessChild;

    fn deref(&self) -> &ProcessChild {
        &self.child
    }
}

impl DerefMut for Child {
    fn deref_mut(&mut self) -> &mut ProcessChild {
        &mut self.child
    }
}

impl Child {
    pub fn wait(&mut self) -> Result<ExitStatus, CliError> {
        self.child.wait()
            .map_err(|err| CliError::SpawnError {
                name: self.name.clone(),
                args: self.args.clone(),
                cause: err
            })
    }

    pub fn wait_success(&mut self) -> Result<(), CliError> {
        self.wait()
            .and_then(|code| {
                if code.success() {
                    Ok(())
                } else {
                    Err(CliError::process_error(&format!("process didn't exit successfully: {:?}", self.name), None, code))
                }
            })
    }
}