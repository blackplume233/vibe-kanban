use std::path::Path;

fn main() {
    // Load .env from the workspace root
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let env_file = workspace_root.join(".env");
    dotenv::from_path(&env_file).ok();

    // Re-run build script when these env vars or .env file change
    println!("cargo:rerun-if-env-changed=POSTHOG_API_KEY");
    println!("cargo:rerun-if-env-changed=POSTHOG_API_ENDPOINT");
    println!("cargo:rerun-if-env-changed=VK_SHARED_API_BASE");
    println!("cargo:rerun-if-env-changed=SENTRY_DSN");
    if env_file.exists() {
        println!("cargo:rerun-if-changed={}", env_file.display());
    }

    if let Ok(api_key) = std::env::var("POSTHOG_API_KEY") {
        println!("cargo:rustc-env=POSTHOG_API_KEY={}", api_key);
    }
    if let Ok(api_endpoint) = std::env::var("POSTHOG_API_ENDPOINT") {
        println!("cargo:rustc-env=POSTHOG_API_ENDPOINT={}", api_endpoint);
    }
    if let Ok(vk_shared_api_base) = std::env::var("VK_SHARED_API_BASE") {
        println!("cargo:rustc-env=VK_SHARED_API_BASE={}", vk_shared_api_base);
    }
    if let Ok(vk_shared_relay_api_base) = std::env::var("VK_SHARED_RELAY_API_BASE") {
        println!(
            "cargo:rustc-env=VK_SHARED_RELAY_API_BASE={}",
            vk_shared_relay_api_base
        );
    }
}
