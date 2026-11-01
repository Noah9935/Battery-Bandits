#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(web::app::App);
}

/// Auf dem Host gibt es nichts zu starten. Die App läuft im Browser (Build mit Trunk).
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("Die Web-App wird mit Trunk nach WebAssembly gebaut: trunk build web/index.html");
}
