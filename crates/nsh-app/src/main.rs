#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> impl std::process::Termination {
    use nsh_platform_windows::{AppOptions, AppSource, Root, run_app, set_ui_sets};

    let fps = std::env::args().any(|arg| arg == "--fps");
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(if fps {
        "warn,nsh::perf=info,vibe_own_nsh=info"
    } else {
        "warn"
    }))
    .init();
    set_ui_sets(vec![nsh_base::registry()]);
    run_app(
        AppSource::Rust(Root::of::<vibe_own_nsh::App>().app_id("io.vibe-own.desktop")),
        AppOptions {
            title: "vibe-own | NSH M1".into(),
            size: (1080, 760),
            fps,
            ..AppOptions::default()
        },
    )
}

#[cfg(not(windows))]
fn main() {}
