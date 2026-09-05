include!("../../../build/version.rs");

fn main() {
    configure_stassh_version();
    tauri_build::build()
}
