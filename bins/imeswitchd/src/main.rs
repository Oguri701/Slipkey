#[cfg(target_os = "macos")]
fn run() -> anyhow::Result<()> {
    use imeswitch_core::Language;
    use imeswitch_macos::config::{self, LoadOutcome};
    use imeswitch_macos::{run_loop, EventHook, ImeSwitcher};
    use std::sync::Arc;

    let (mapping, outcome) = config::load_or_default();
    match &outcome {
        LoadOutcome::Loaded { path, .. } => {
            log::info!("config loaded from {}", path.display());
        }
        LoadOutcome::Missing { path } => {
            log::info!(
                "no config at {} — using built-in defaults (run `imeswitchd init` to write a template)",
                path.display()
            );
        }
        LoadOutcome::ParseError { .. } => { /* already warned inside load */ }
    }
    log::info!(
        "mapping: en={} ja={} zh={}",
        mapping.en, mapping.ja, mapping.zh
    );

    let switcher = Arc::new(ImeSwitcher::with_mapping(mapping));
    let switcher_cb = switcher.clone();
    let _hook = EventHook::install(move |lang: Language| {
        let before = imeswitch_macos::ime::current_source_id();
        let result = switcher_cb.switch_to(lang);
        let after = imeswitch_macos::ime::current_source_id();
        match result {
            Ok(()) => log::info!(
                "switch {:?}: {} -> {}",
                lang,
                before.as_deref().unwrap_or("<none>"),
                after.as_deref().unwrap_or("<none>"),
            ),
            Err(e) => log::error!(
                "switch {:?} failed: {} (was: {})",
                lang,
                e,
                before.as_deref().unwrap_or("<none>"),
            ),
        }
    })
    .map_err(|e| anyhow::anyhow!("hook install failed: {e}. Check System Settings → Privacy & Security → Accessibility and grant this binary permission, then relaunch."))?;

    log::info!(
        "imeswitchd running. Triggers: ;en ;ja ;zh. Press Ctrl-C to stop."
    );
    run_loop();
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn run() -> anyhow::Result<()> {
    anyhow::bail!("imeswitchd currently only supports macOS (M0). Windows support lands in M1.")
}

#[cfg(target_os = "macos")]
fn list_sources() {
    let sources = imeswitch_macos::ime::list_all_sources();
    println!("# {} input sources reported by TIS", sources.len());
    println!("# id | category | type | enabled | selectable | name");
    for s in sources {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            s.id, s.category, s.type_, s.is_enabled, s.is_selectable, s.name
        );
    }
}

#[cfg(target_os = "macos")]
fn init_config() -> anyhow::Result<()> {
    use imeswitch_macos::config::{default_path, Config};

    let path = default_path();
    if path.exists() {
        anyhow::bail!(
            "{} already exists — refusing to overwrite",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, Config::template_toml())?;
    println!("wrote {}", path.display());
    println!("edit it and restart imeswitchd; use `imeswitchd list` to find IDs for your IMEs.");
    Ok(())
}

fn print_usage() {
    eprintln!("usage: imeswitchd [SUBCOMMAND]");
    eprintln!();
    eprintln!("Subcommands:");
    eprintln!("  (none)  run the daemon (default)");
    eprintln!("  list    print all input sources known to TIS");
    eprintln!("  init    write a starter config at ~/.config/imeswitch/config.toml");
}

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
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
            #[cfg(target_os = "macos")]
            list_sources();
            #[cfg(not(target_os = "macos"))]
            {
                eprintln!("list only supported on macOS");
                std::process::exit(1);
            }
        }
        Some("init") => {
            #[cfg(target_os = "macos")]
            {
                if let Err(e) = init_config() {
                    eprintln!("init failed: {e:#}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                eprintln!("init only supported on macOS");
                std::process::exit(1);
            }
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
