#[cfg(target_os = "windows")]
fn run() -> anyhow::Result<()> {
    use imeswitch_core::Language;
    use imeswitch_windows::config::{self, LoadOutcome};
    use imeswitch_windows::keymap::{leader_vk_for, VK_SEMICOLON};
    use imeswitch_windows::{run_loop, EventHook, ImeSwitcher};
    use std::sync::Arc;

    let (mapping, outcome) = config::load_or_default();
    match &outcome {
        LoadOutcome::Loaded { path, .. } => {
            log::info!("config loaded from {}", path.display());
        }
        LoadOutcome::Missing { path } => {
            log::info!(
                "no config at {} - using built-in defaults (run `imeswitchd init` to write a template)",
                path.display()
            );
        }
        LoadOutcome::Migrated {
            path, backup_path, ..
        } => {
            log::info!(
                "migrated {} to config v2; backup written to {}",
                path.display(),
                backup_path.display()
            );
        }
        LoadOutcome::ParseError { .. } => { /* already warned inside load */ }
    }
    log::info!("mapping: {}", mapping.describe());

    let trigger_mappings = mapping.trigger_mappings();
    let leader_vk = leader_vk_for(mapping.leader()).unwrap_or(VK_SEMICOLON);
    let switcher = Arc::new(ImeSwitcher::with_mapping(mapping));
    let switcher_cb = switcher.clone();
    let _hook =
        EventHook::install_with_mappings(leader_vk, trigger_mappings, move |lang: Language| {
            let before = imeswitch_windows::ime::current_source_id();
            let result = switcher_cb.switch_to(&lang);
            let after = imeswitch_windows::ime::current_source_id();
            match result {
                Ok(()) => log::info!(
                    "switch {}: {} -> {}",
                    lang,
                    before.as_deref().unwrap_or("<none>"),
                    after.as_deref().unwrap_or("<none>"),
                ),
                Err(e) => log::error!(
                    "switch {} failed: {} (was: {})",
                    lang,
                    e,
                    before.as_deref().unwrap_or("<none>"),
                ),
            }
        })
        .map_err(|e| anyhow::anyhow!("hook install failed: {e}"))?;

    log::info!("imeswitchd running. Press Ctrl-C to stop.");
    run_loop();
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn run() -> anyhow::Result<()> {
    anyhow::bail!("imeswitchd is Windows-only. On macOS, use the Slipkey app instead.")
}

#[cfg(target_os = "windows")]
fn list_sources() {
    let sources = imeswitch_windows::ime::list_all_sources();
    println!("# {} keyboard layouts reported by Windows", sources.len());
    println!("# id");
    for s in sources {
        println!("{}", s.id);
    }
}

#[cfg(target_os = "windows")]
fn init_config() -> anyhow::Result<()> {
    use imeswitch_windows::config::{default_path, Config};

    let path = default_path();
    if path.exists() {
        anyhow::bail!("{} already exists - refusing to overwrite", path.display());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, Config::template_toml())?;
    println!("wrote {}", path.display());
    println!("edit it and restart imeswitchd; use `imeswitchd list` to find installed HKL IDs.");
    Ok(())
}

fn print_usage() {
    eprintln!("usage: imeswitchd [SUBCOMMAND]");
    eprintln!();
    eprintln!("On macOS, IME switching now lives inside the Slipkey app — this");
    eprintln!("CLI is Windows-only.");
    eprintln!();
    eprintln!("Subcommands:");
    eprintln!("  (none)  run the daemon (default; Windows only)");
    eprintln!("  list    print all keyboard layouts known to the OS (Windows only)");
    eprintln!("  init    write a starter config file (Windows only)");
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        None => {
            if let Err(e) = run() {
                eprintln!("fatal: {e:#}");
                std::process::exit(1);
            }
        }
        Some("list") | Some("--list") => {
            #[cfg(target_os = "windows")]
            list_sources();
            #[cfg(not(target_os = "windows"))]
            {
                eprintln!("list only supported on Windows");
                std::process::exit(1);
            }
        }
        Some("init") => {
            #[cfg(target_os = "windows")]
            {
                if let Err(e) = init_config() {
                    eprintln!("init failed: {e:#}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                eprintln!("init only supported on Windows");
                std::process::exit(1);
            }
        }
        Some("wizard") => {
            eprintln!("wizard is no longer supported in this CLI; use the Slipkey app");
            std::process::exit(1);
        }
        Some("-h") | Some("--help") | Some("help") => {
            print_usage();
        }
        Some(other) => {
            eprintln!("unknown subcommand: {other}");
            print_usage();
            std::process::exit(2);
        }
    }
}
