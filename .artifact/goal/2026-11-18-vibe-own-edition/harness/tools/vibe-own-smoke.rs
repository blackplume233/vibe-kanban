//! M1 探针的 Win32 入口。

#![windows_subsystem = "windows"]

use nsh_platform_windows::{AppOptions, AppSource, Appearance, Root, run_app};

include!("vibe-own-root.rs");

fn main() -> std::process::ExitCode {
    run_app(
        AppSource::Rust(Root::of::<Smoke>()),
        AppOptions {
            app_id: "dev.vibe-own.m1-smoke".to_owned(),
            title: "vibe-own | NSH M1".to_owned(),
            appearance: Appearance::Light,
            size: (540, 240),
            ..Default::default()
        },
    )
}
