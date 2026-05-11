use std::path::{Path, PathBuf};
use std::process::Command;

const TARGET: &str = "x86_64-pc-windows-msvc";
const HELPER_DLL: &str = "target/x86_64-pc-windows-msvc/release/slipkey_tsf_helper.dll";
const EMBED_DLL: &str = "bins/slipkey-windows/embed/slipkey_tsf.dll";

fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        Some("stage-windows-helper") => stage_windows_helper(),
        Some("build-windows") => build_windows(),
        Some("test-windows") => test_windows(),
        Some("-h") | Some("--help") | None => {
            print_help();
            Ok(())
        }
        Some(other) => Err(format!("unknown xtask command: {other}")),
    };

    if let Err(error) = result {
        eprintln!("xtask failed: {error}");
        std::process::exit(1);
    }
}

fn print_help() {
    eprintln!("Usage:");
    eprintln!("  cargo xtask stage-windows-helper");
    eprintln!("  cargo xtask build-windows");
    eprintln!("  cargo xtask test-windows");
}

fn stage_windows_helper() -> Result<(), String> {
    cargo([
        "build",
        "--release",
        "-p",
        "slipkey-tsf-helper",
        "--target",
        TARGET,
    ])?;

    let src = repo_path(HELPER_DLL);
    let dst = repo_path(EMBED_DLL);
    let parent = dst
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", dst.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("create {}: {error}", parent.display()))?;
    std::fs::copy(&src, &dst)
        .map_err(|error| format!("copy {} -> {}: {error}", src.display(), dst.display()))?;
    println!("staged helper DLL: {}", dst.display());
    Ok(())
}

fn build_windows() -> Result<(), String> {
    stage_windows_helper()?;
    cargo([
        "build",
        "--release",
        "-p",
        "slipkey-windows",
        "--target",
        TARGET,
    ])
}

fn test_windows() -> Result<(), String> {
    stage_windows_helper()?;
    cargo(["test", "--workspace"])
}

fn cargo<const N: usize>(args: [&str; N]) -> Result<(), String> {
    run("cargo", &args)
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|error| format!("spawn {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} {} exited with {status}", args.join(" ")))
    }
}

fn repo_path(path: impl AsRef<Path>) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest dir has workspace parent")
        .join(path)
}
