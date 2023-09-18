use chibiwasm::wasi::file::File;
use containerd_shim_wasm::sandbox::oci;
use libc::{STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO};
use libcontainer::workload::{Executor, ExecutorError};
use nix::unistd::{dup, dup2};
use oci_spec::runtime::Spec;
use std::{
    os::fd::RawFd,
    sync::{Arc, Mutex},
};

const EXECUTOR_NAME: &str = "chibiwasm";

pub struct ChibiwasmExecutor {
    pub stdin: Option<RawFd>,
    pub stdout: Option<RawFd>,
    pub stderr: Option<RawFd>,
}

impl Executor for ChibiwasmExecutor {
    fn exec(&self, spec: &Spec) -> Result<(), ExecutorError> {
        let args = oci::get_args(spec);
        if args.is_empty() {
            return Err(ExecutorError::InvalidArg);
        }

        if let Some(stdin) = self.stdin {
            dup(STDIN_FILENO).unwrap();
            dup2(stdin, STDIN_FILENO).unwrap();
        }
        if let Some(stdout) = self.stdout {
            dup(STDOUT_FILENO).unwrap();
            dup2(stdout, STDOUT_FILENO).unwrap();
        }
        if let Some(stderr) = self.stderr {
            dup(STDERR_FILENO).unwrap();
            dup2(stderr, STDERR_FILENO).unwrap();
        }

        let mut iterator = args
            .first()
            .expect("args must have at least one argument.")
            .split('#');

        let mut cmd = iterator.next().unwrap().to_string();
        let stripped = cmd.strip_prefix(std::path::MAIN_SEPARATOR);
        if let Some(strpd) = stripped {
            cmd = strpd.to_string();
        }
        let method = iterator.next().unwrap_or("_start");
        let mod_path = cmd;

        let io = vec![
            Arc::new(Mutex::new(
                File::from_raw_fd(self.stdin.unwrap_or(0) as u32),
            )),
            Arc::new(Mutex::new(File::from_raw_fd(
                self.stdout.unwrap_or(1) as u32
            ))),
            Arc::new(Mutex::new(File::from_raw_fd(
                self.stderr.unwrap_or(2) as u32
            ))),
        ];

        let mut runtime = chibiwasm::Runtime::from_file(
            &mod_path,
            Some(Box::new(
                chibiwasm::wasi::preview1::WasiSnapshotPreview1::with_io(io),
            )),
        )
        .expect("failed to create chibiwasm runtime");

        match runtime.call(method.into(), vec![]) {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("failed call: {}. error: {}", method, e);
                std::process::exit(137)
            }
        };
    }

    fn can_handle(&self, _spec: &Spec) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        EXECUTOR_NAME
    }
}
