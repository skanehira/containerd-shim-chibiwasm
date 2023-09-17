use containerd_shim as shim;
use containerd_shim_wasm::sandbox::{
    self, instance::Wait, EngineGetter, Instance, InstanceConfig, ShimCli,
};

struct ChibiwasmInstance {}

unsafe impl Send for ChibiwasmInstance {}
unsafe impl Sync for ChibiwasmInstance {}

#[derive(Clone)]
struct Engine {
    inner: chibiwasm::Runtime,
}

unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}

impl EngineGetter for ChibiwasmInstance {
    type E = Engine;
    fn new_engine() -> Result<Self::E, sandbox::Error> {
        Ok(Engine {
            inner: chibiwasm::Runtime::default(),
        })
    }
}

impl Instance for ChibiwasmInstance {
    type E = Engine;

    fn new(id: String, cfg: Option<&InstanceConfig<Self::E>>) -> Self {
        todo!()
    }

    fn start(&self) -> Result<u32, sandbox::Error> {
        todo!()
    }

    fn kill(&self, signal: u32) -> Result<(), sandbox::Error> {
        todo!()
    }

    fn delete(&self) -> Result<(), sandbox::Error> {
        todo!()
    }

    fn wait(&self, waiter: &Wait) -> Result<(), sandbox::Error> {
        todo!()
    }
}

fn main() {
    shim::run::<ShimCli<ChibiwasmInstance, Engine>>("io.containerd.chibiwasm.v1", None);
}
