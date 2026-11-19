#[cfg(any(windows, target_arch = "wasm32"))]
mod api;
#[cfg(any(windows, target_arch = "wasm32"))]
mod app;
#[cfg(windows)]
mod desktop;
mod package;
#[cfg(any(windows, target_arch = "wasm32"))]
mod board_ui;
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
fn native(registry: &mut nsh_core::element::Registry) {
    use nsh_core::element::{AssetFile, AssetKind};

    registry.assets(&[
        AssetFile {
            uri: "asset:/@vibe-own-nsh/fonts/SourceSans3.ttf",
            source: concat!(env!("CARGO_MANIFEST_DIR"), "/design/fonts/SourceSans3.ttf"),
            kind: AssetKind::Font,
        },
        AssetFile {
            uri: "asset:/@vibe-own-nsh/fonts/IBMPlexMono-Regular.ttf",
            source: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/design/fonts/IBMPlexMono-Regular.ttf"
            ),
            kind: AssetKind::Font,
        },
        AssetFile {
            uri: "asset:/@vibe-own-nsh/fonts/SourceSans3-OFL.txt",
            source: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/design/fonts/SourceSans3-OFL.txt"
            ),
            kind: AssetKind::File,
        },
        AssetFile {
            uri: "asset:/@vibe-own-nsh/fonts/IBMPlexMono-OFL.txt",
            source: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/design/fonts/IBMPlexMono-OFL.txt"
            ),
            kind: AssetKind::File,
        },
    ]);
}

#[cfg(all(target_arch = "wasm32", feature = "m1-web-smoke"))]
nsh_platform_web::rust_app!(
    vec![nsh_base::registry()],
    nsh_core::element::Root::of::<App>().app_id("io.vibe-own.desktop")
);
