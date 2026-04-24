#[cfg(target_os = "macos")]
fn run() -> anyhow::Result<()> {
    use imeswitch_core::Language;
    use imeswitch_macos::{run_loop, EventHook, ImeSwitcher};
    use std::sync::Arc;

    let switcher = Arc::new(ImeSwitcher::new());
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

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .format_timestamp_secs()
    .init();

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "list" || a == "--list") {
        #[cfg(target_os = "macos")]
        {
            list_sources();
            return;
        }
        #[cfg(not(target_os = "macos"))]
        {
            eprintln!("list only supported on macOS");
            std::process::exit(1);
        }
    }

    if let Err(e) = run() {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}
