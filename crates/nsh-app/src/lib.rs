#[cfg(any(windows, target_arch = "wasm32"))]
mod api;
#[cfg(any(windows, target_arch = "wasm32"))]
mod app;
#[cfg(windows)]
mod desktop;
mod package;
#[cfg(any(windows, target_arch = "wasm32"))]
mod socket;

pub mod board_state;
pub mod command_search;
pub mod issue_actions;
pub mod issues_api;

#[cfg(any(windows, target_arch = "wasm32"))]
pub use api::Api;
#[cfg(any(windows, target_arch = "wasm32"))]
pub use app::App;
pub use package::PackagedApp;

#[nsh_core::prelude::native_module("vibe-own/native")]
fn native(_registry: &mut nsh_core::element::Registry) {}

#[cfg(all(target_arch = "wasm32", feature = "m1-web-smoke"))]
nsh_platform_web::rust_app!(
    vec![nsh_base::registry()],
    nsh_core::element::Root::of::<App>().app_id("io.vibe-own.desktop")
);
