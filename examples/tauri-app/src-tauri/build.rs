fn main() {
    // Downloads libgaiadesk_embed for the target into gaiadesk/ (SHA-256 checked, once),
    // before tauri-build looks for the bundle's resources.
    gaiadesk_embed_fetch::fetch("gaiadesk").expect("libgaiadesk_embed");
    tauri_build::build();
}
