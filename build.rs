const COMMANDS: &[&str] = &["frontend_ack"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
