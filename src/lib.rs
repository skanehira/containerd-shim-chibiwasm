mod executor;
use anyhow::Context;
use chrono::{DateTime, Utc};
use containerd_shim_wasm::sandbox::{
    self,
    instance::Wait,
    instance_utils::{get_instance_root, instance_exists, maybe_open_stdio},
    EngineGetter, Instance, InstanceConfig,
};
use executor::ChibiwasmExecutor;
use libc::{dup2, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO};
use libcontainer::{
    container::{builder::ContainerBuilder, Container},
    syscall::syscall::create_syscall,
};
use nix::sys::wait::waitid;
use nix::{
    errno::Errno,
    sys::wait::{Id as WaitID, WaitPidFlag, WaitStatus},
};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    os::fd::RawFd,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
};

static DEFAULT_CONTAINER_ROOT_DIR: &str = "/run/containerd/chibiwasm";

type ExitCode = Arc<(Mutex<Option<(u32, DateTime<Utc>)>>, Condvar)>;

static mut STDIN_FD: Option<RawFd> = None;
static mut STDOUT_FD: Option<RawFd> = None;
static mut STDERR_FD: Option<RawFd> = None;

pub fn reset_stdio() {
    unsafe {
        if STDIN_FD.is_some() {
            dup2(STDIN_FD.unwrap(), STDIN_FILENO);
        }
        if STDOUT_FD.is_some() {
            dup2(STDOUT_FD.unwrap(), STDOUT_FILENO);
        }
        if STDERR_FD.is_some() {
            dup2(STDERR_FD.unwrap(), STDERR_FILENO);
        }
    }
}

pub struct ChibiwasmInstance {
    id: String,
    exit_code: ExitCode,
    stdin: String,
    stdout: String,
    stderr: String,
    bundle: String,
    rootdir: PathBuf,
}

#[derive(Clone, Default)]
pub struct Engine {}

impl EngineGetter for ChibiwasmInstance {
    type E = Engine;
    fn new_engine() -> Result<Self::E, sandbox::Error> {
        Ok(Engine {})
    }
}

#[derive(Serialize, Deserialize)]
struct RootPath {
    path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Options {
    root: RootPath,
}

fn determine_rootdir<P: AsRef<Path>>(
    bundle: P,
    namespace: String,
) -> Result<PathBuf, sandbox::Error> {
    let mut file = match std::fs::File::open(bundle.as_ref().join("config.json")) {
        Ok(f) => f,
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => {
                eprintln!("not found config.json");
                return Ok(PathBuf::from(DEFAULT_CONTAINER_ROOT_DIR).join(namespace));
            }
            _ => return Err(sandbox::Error::NotFound("config.json".into())),
        },
    };
    let mut data = String::new();
    file.read_to_string(&mut data)?;
    let options: Options = serde_json::from_str(&data)?;
    let path = options.root.path.join(namespace);

    Ok(path)
}

impl Instance for ChibiwasmInstance {
    type E = Engine;

    fn new(id: String, cfg: Option<&InstanceConfig<Self::E>>) -> Self {
        let cfg = cfg.unwrap();
        // https://zenn.dev/nokute/articles/0a2cfe8ebcd6c7636a0d
        let bundle = cfg.get_bundle().unwrap_or_default();
        let rootdir = determine_rootdir(bundle.as_str(), cfg.get_namespace()).unwrap();

        Self {
            id,
            exit_code: Arc::new((Mutex::new(None), Condvar::new())),
            stdin: cfg.get_stdin().unwrap_or_default(),
            stdout: cfg.get_stdout().unwrap_or_default(),
            stderr: cfg.get_stderr().unwrap_or_default(),
            bundle,
            rootdir,
        }
    }

    fn start(&self) -> Result<u32, sandbox::Error> {
        let mut container = self.build_container(
            self.stdin.as_str(),
            self.stdout.as_str(),
            self.stderr.as_str(),
        );

        let code = self.exit_code.clone();
        let pid = container.pid().unwrap();

        container.start().map_err(|err| {
            sandbox::Error::Any(anyhow::anyhow!("failed to start container: {}", err))
        })?;

        thread::spawn(move || {
            let (lock, cvar) = &*code;

            let status = match waitid(WaitID::Pid(pid), WaitPidFlag::WEXITED) {
                Ok(WaitStatus::Exited(_, status)) => status,
                Ok(WaitStatus::Signaled(_, sig, _)) => sig as i32,
                Ok(_) => 0,
                Err(e) => {
                    if e == Errno::ECHILD {
                        eprintln!("no child process");
                        0
                    } else {
                        panic!("waitpid failed: {}", e);
                    }
                }
            } as u32;

            let mut ec = lock.lock().unwrap();
            *ec = Some((status, Utc::now()));
            drop(ec);
            cvar.notify_all();
        });

        Ok(pid.as_raw() as u32)
    }

    fn kill(&self, _signal: u32) -> Result<(), sandbox::Error> {
        todo!()
    }

    fn delete(&self) -> Result<(), sandbox::Error> {
        match instance_exists(&self.rootdir, self.id.as_str()) {
            Ok(exists) => {
                if !exists {
                    return Ok(());
                }
            }
            Err(err) => {
                eprintln!("could not find the container, skipping cleanup: {}", err);
                return Ok(());
            }
        }
        let container_root = get_instance_root(&self.rootdir, self.id.as_str())?;
        let container = Container::load(container_root).with_context(|| {
            format!(
                "could not load state for container {id}",
                id = self.id.as_str()
            )
        });
        match container {
            Ok(mut container) => container.delete(true).map_err(|err| {
                sandbox::Error::Any(anyhow::anyhow!(
                    "failed to delete container {}: {}",
                    self.id,
                    err
                ))
            })?,
            Err(err) => {
                eprintln!("could not find the container, skipping cleanup: {}", err);
                return Ok(());
            }
        }

        Ok(())
    }

    fn wait(&self, waiter: &Wait) -> Result<(), sandbox::Error> {
        eprintln!("waiting for instance: {}", self.id);
        let code = self.exit_code.clone();
        waiter.set_up_exit_code_wait(code)
    }
}

impl ChibiwasmInstance {
    fn build_container(&self, stdin: &str, stdout: &str, stderr: &str) -> Container {
        let syscall = create_syscall();
        let stdin = maybe_open_stdio(stdin).expect("could not open stdin");
        let stdout = maybe_open_stdio(stdout).expect("could not open stdout");
        let stderr = maybe_open_stdio(stderr).expect("could not open stderr");

        let container = ContainerBuilder::new(self.id.clone(), syscall.as_ref())
            .with_executor(vec![Box::new(ChibiwasmExecutor {
                stdin,
                stdout,
                stderr,
            })])
            .expect("could not create executor")
            .with_root_path(self.rootdir.clone())
            .expect("could not set root path")
            .as_init(&self.bundle)
            .with_systemd(false)
            .build()
            .expect("could not build container");
        container
    }
}

#[cfg(test)]
mod test {
    use containerd_shim_wasm::sandbox::instance::Wait;
    use containerd_shim_wasm::sandbox::Error;
    use nix::sys::signal::Signal::SIGKILL;
    use oci_spec::runtime::{ProcessBuilder, RootBuilder, SpecBuilder};
    use std::fs::{create_dir, read_to_string, File, OpenOptions};
    use std::io::prelude::*;
    use std::os::unix::prelude::OpenOptionsExt;
    use std::sync::mpsc::channel;
    use std::time::Duration;
    use tempfile::{tempdir, TempDir};

    use super::*;
    #[test]
    fn test_wasi() -> Result<(), Error> {
        let dir = tempdir()?;
        let cfg = prepare_cfg(&dir)?;

        let wasi = ChibiwasmInstance::new("test".to_string(), Some(&cfg));

        wasi.start()?;

        let (tx, rx) = channel();
        let waiter = Wait::new(tx);
        wasi.wait(&waiter).unwrap();

        let res = match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(res) => res,
            Err(e) => {
                wasi.kill(SIGKILL as u32).unwrap();
                return Err(Error::Others(format!(
                    "error waiting for module to finish: {0}",
                    e
                )));
            }
        };
        assert_eq!(res.0, 0);

        let output = read_to_string(dir.path().join("stdout"))?;
        assert_eq!(output, "hello world\n");

        wasi.delete()?;

        reset_stdio();
        Ok(())
    }

    fn prepare_cfg(dir: &TempDir) -> anyhow::Result<InstanceConfig<Engine>> {
        create_dir(dir.path().join("rootfs"))?;

        let opts = Options {
            root: RootPath {
                path: dir.path().join("runwasi"),
            },
        };
        let opts_file = OpenOptions::new()
            .read(true)
            .create(true)
            .truncate(true)
            .write(true)
            .open(dir.path().join("options.json"))?;
        write!(&opts_file, "{}", serde_json::to_string(&opts)?)?;

        let wasm_path = dir.path().join("rootfs/hello.wasm");
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o755)
            .open(wasm_path.clone())?;
        std::io::copy(&mut File::open("src/fixtures/hello.wasm")?, &mut f)?;

        let stdout = File::create(dir.path().join("stdout"))?;
        let stderr = File::create(dir.path().join("stderr"))?;
        drop(stdout);
        drop(stderr);
        let spec = SpecBuilder::default()
            .root(RootBuilder::default().path("rootfs").build()?)
            .process(
                ProcessBuilder::default()
                    .cwd("/")
                    .args(vec!["./hello.wasm".to_string()])
                    .build()?,
            )
            .build()?;
        spec.save(dir.path().join("config.json"))?;
        let mut cfg = InstanceConfig::new(
            Engine::default(),
            "test_namespace".into(),
            "/containerd/address".into(),
        );
        let cfg = cfg
            .set_bundle(dir.path().to_str().unwrap().to_string())
            .set_stdout(dir.path().join("stdout").to_str().unwrap().to_string())
            .set_stderr(dir.path().join("stderr").to_str().unwrap().to_string());
        Ok(cfg.to_owned())
    }
}
