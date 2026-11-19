#[cfg(any(windows, target_arch = "wasm32"))]
mod api;
#[cfg(any(windows, target_arch = "wasm32"))]
mod app;
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

#[cfg(target_arch = "wasm32")]
nsh_platform_web::rust_app!(
    vec![nsh_base::registry()],
    nsh_core::element::Root::of::<App>().app_id("io.vibe-own.desktop")
);
