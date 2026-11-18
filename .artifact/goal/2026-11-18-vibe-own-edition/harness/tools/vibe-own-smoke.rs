//! M1 探针。用锁定版本的 NSH Rust 根组件验证 Win32 渲染。

#![windows_subsystem = "windows"]

use nsh_core::prelude::*;
use nsh_platform_windows::{AppOptions, AppSource, Appearance, Root, run_app};

#[component]
fn Smoke() -> impl IntoElement {
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .child(text("NSH 原生窗口已启动").text_size(px(28.0)))
}

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
