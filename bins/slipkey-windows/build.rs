//! Preflight: verify embed/slipkey_tsf.dll exists before compiling main.rs.
//!
//! The DLL is produced by the slipkey-tsf-helper crate and copied here by the
//! release workflow / local build script. Without it, include_bytes!() in
//! dll_provisioning.rs would fail with a cryptic error.

use std::path::Path;

fn main() {
    let dll_path = Path::new("embed/slipkey_tsf.dll");
    println!("cargo:rerun-if-changed=embed/slipkey_tsf.dll");

    if !dll_path.exists() {
        eprintln!();
        eprintln!("Missing: bins/slipkey-windows/embed/slipkey_tsf.dll");
        eprintln!();
        eprintln!("Build the helper crate first, then copy the DLL into embed/:");
        eprintln!();
        eprintln!("  cargo build --release -p slipkey-tsf-helper --target x86_64-pc-windows-msvc");
        eprintln!("  Copy-Item target/x86_64-pc-windows-msvc/release/slipkey_tsf_helper.dll \\");
        eprintln!("    bins/slipkey-windows/embed/slipkey_tsf.dll -Force");
        eprintln!();
        std::process::exit(1);
    }
}
