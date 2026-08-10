fn main() {
    tauri_build::build();

    let client_id = std::env::var("SKINARTPLUS_CLIENT_ID")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb".to_string());
    println!("cargo:rustc-env=SKINARTPLUS_CLIENT_ID={}", client_id);
    println!("cargo:rerun-if-env-changed=SKINARTPLUS_CLIENT_ID");

    if let Ok(secret) = std::env::var("SKINARTPLUS_CLIENT_SECRET") {
        if !secret.is_empty() {
            println!("cargo:rustc-env=SKINARTPLUS_CLIENT_SECRET={}", secret);
        }
    }
    println!("cargo:rerun-if-env-changed=SKINARTPLUS_CLIENT_SECRET");

    let authority = std::env::var("SKINARTPLUS_MS_AUTHORITY")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "consumers".to_string());
    println!("cargo:rustc-env=SKINARTPLUS_MS_AUTHORITY={}", authority);
    println!("cargo:rerun-if-env-changed=SKINARTPLUS_MS_AUTHORITY");
}
