#[cfg(target_os = "macos")]
fn run() -> anyhow::Result<()> {
    use imeswitch_core::Language;
    use imeswitch_macos::config::{self, LoadOutcome};
    use imeswitch_macos::keymap::{leader_keycode_for, KC_SEMICOLON};
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
    let leader_keycode = leader_keycode_for(mapping.leader()).unwrap_or(KC_SEMICOLON);
    let switcher = Arc::new(ImeSwitcher::with_mapping(mapping));
    let switcher_cb = switcher.clone();
    let _hook = EventHook::install_with_mappings(leader_keycode, trigger_mappings, move |lang: Language| {
        let before = imeswitch_macos::ime::current_source_id();
        let result = switcher_cb.switch_to(&lang);
        let after = imeswitch_macos::ime::current_source_id();
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
    .map_err(|e| anyhow::anyhow!("hook install failed: {e}. Check System Settings → Privacy & Security → Accessibility and grant this binary permission, then relaunch."))?;

    log::info!("imeswitchd running. Press Ctrl-C to stop.");
    run_loop();
    Ok(())
}

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
    let _hook = EventHook::install_with_mappings(leader_vk, trigger_mappings, move |lang: Language| {
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

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn run() -> anyhow::Result<()> {
    anyhow::bail!("imeswitchd currently supports macOS and Windows.")
}

#[cfg(target_os = "macos")]
fn list_sources() {
    let sources = imeswitch_macos::ime::list_all_sources();
    println!("# {} input sources reported by TIS", sources.len());
    println!("# id | category | type | enabled | selectable | languages | name");
    for s in sources {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            s.id,
            s.category,
            s.type_,
            s.is_enabled,
            s.is_selectable,
            s.languages.join(","),
            s.name
        );
    }
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

#[cfg(target_os = "macos")]
fn init_config() -> anyhow::Result<()> {
    use imeswitch_macos::config::{default_path, Config};

    let path = default_path();
    if path.exists() {
        anyhow::bail!("{} already exists — refusing to overwrite", path.display());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, Config::template_toml())?;
    println!("wrote {}", path.display());
    println!("edit it and restart imeswitchd; use `imeswitchd list` to find IDs for your IMEs.");
    Ok(())
}

#[cfg(target_os = "macos")]
fn wizard_config() -> anyhow::Result<()> {
    use imeswitch_macos::config::{default_path, Config};
    use imeswitch_macos::ime::{discover_installed_imes, Mapping, MappingEntry};
    use std::collections::BTreeMap;

    let path = default_path();
    if path.exists() {
        anyhow::bail!(
            "{} already exists — refusing to overwrite; move it aside to rerun wizard",
            path.display()
        );
    }

    let detected = discover_installed_imes();
    if detected.is_empty() {
        anyhow::bail!("no enabled selectable input sources with language metadata were detected");
    }

    let mut by_language = BTreeMap::new();
    for item in detected {
        by_language
            .entry(item.language.clone())
            .or_insert_with(Vec::new)
            .push(item);
    }

    let mut entries = Vec::new();
    println!("# detected input sources");
    for (language, candidates) in by_language {
        println!();
        println!("[{}]", language);
        for (index, candidate) in candidates.iter().enumerate() {
            let marker = if index == 0 { "*" } else { " " };
            println!(
                "{} {} ({}) {}",
                marker, candidate.name, candidate.source_id, candidate.is_selectable
            );
        }
        if let Some(first) = candidates.first() {
            entries.push(MappingEntry {
                language: language.clone(),
                prefix: language.to_string(),
                source: first.source_id.clone(),
            });
        }
    }

    let config = Config::from_mapping(&Mapping::new(entries));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, toml::to_string_pretty(&config)?)?;
    println!();
    println!("wrote {}", path.display());
    println!("review it, then restart imeswitchd.");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn wizard_config() -> anyhow::Result<()> {
    anyhow::bail!("wizard currently only supports macOS TIS input source discovery")
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
    eprintln!("Subcommands:");
    eprintln!("  (none)  run the daemon (default)");
    eprintln!("  list    print all input sources / keyboard layouts known to the OS");
    eprintln!("  init    write a starter config file");
    eprintln!("  wizard  detect enabled macOS input sources and write config v2");
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
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            list_sources();
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                eprintln!("list only supported on macOS and Windows");
                std::process::exit(1);
            }
        }
        Some("init") => {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            {
                if let Err(e) = init_config() {
                    eprintln!("init failed: {e:#}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                eprintln!("init only supported on macOS and Windows");
                std::process::exit(1);
            }
        }
        Some("wizard") => {
            if let Err(e) = wizard_config() {
                eprintln!("wizard failed: {e:#}");
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
