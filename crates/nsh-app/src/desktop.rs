use tokio::{runtime::Runtime, task::JoinHandle};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
pub(crate) struct BackendPort(pub(crate) u16);

pub(crate) struct Backend {
    port: u16,
    runtime: Option<Runtime>,
    shutdown: CancellationToken,
    server: JoinHandle<()>,
}

impl Backend {
    pub(crate) fn start() -> Result<Self, String> {
        // NSH and the server can link different rustls providers. Install the
        // one used by the standalone server before either makes TLS requests.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let runtime = Runtime::new().map_err(|error| error.to_string())?;
        let shutdown = CancellationToken::new();
        let handle = match runtime.block_on(server::startup::start_with_bind(
            "127.0.0.1:0",
            "127.0.0.1:0",
            shutdown.clone(),
        )) {
            Ok(handle) => handle,
            Err(error) => {
                shutdown.cancel();
                runtime.shutdown_background();
                return Err(format!("{error:#}"));
            }
        };
        let port = handle.port;
        let server = runtime.spawn(async move {
            if let Err(error) = handle.serve().await {
                log::error!("Local server failed: {error:#}");
            }
        });
        Ok(Self {
            port,
            runtime: Some(runtime),
            shutdown,
            server,
        })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.shutdown.cancel();
        let runtime = self.runtime.take().expect("backend runtime is live");
        if let Err(error) = runtime.block_on(&mut self.server) {
            log::error!("Local server shutdown failed: {error}");
        }
        // `serve` has cleaned the execution processes. Startup's background
        // cache warmers can still hold blocking Tokio work as the app exits.
        runtime.shutdown_background();
    }
}
