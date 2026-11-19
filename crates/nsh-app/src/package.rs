use nsh_core::prelude::*;

/// The CLI-generated runners mount this component on both platforms.
#[component]
pub fn PackagedApp() -> impl IntoElement {
    #[cfg(windows)]
    {
        use crate::desktop::{Backend, BackendPort};

        if std::env::var_os("VK_NSH_BENCHMARK").is_some_and(|value| value == "1") {
            log::set_max_level(log::LevelFilter::Debug);
        }

        let backend = match Backend::start() {
            Ok(backend) => backend,
            Err(error) => {
                log::error!("Could not start the local server: {error}");
                return text(format!("Could not start the local server: {error}")).into_element();
            }
        };
        provide_context(BackendPort(backend.port()));
        cleanup(move || drop(backend));
    }

    #[cfg(any(windows, target_arch = "wasm32"))]
    return crate::App::element(()).into_element();

    #[cfg(not(any(windows, target_arch = "wasm32")))]
    text("vibe-own supports Windows and Web").into_element()
}
