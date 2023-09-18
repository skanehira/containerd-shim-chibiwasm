use containerd_shim as shim;
use containerd_shim_chibiwasm::{ChibiwasmInstance, Engine};
use containerd_shim_wasm::sandbox::ShimCli;

fn main() {
    shim::run::<ShimCli<ChibiwasmInstance, Engine>>("io.containerd.chibiwasm.v1", None);
}
