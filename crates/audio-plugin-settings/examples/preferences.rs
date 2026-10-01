//! Inspect the shared configuration, or create it without replacing existing
//! keys. This example uses the same locking/atomic-write protocol as plugins.
use audio_plugin_settings::{Store, settings_path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::new(settings_path("com.aflocat.audio")?);
    let document = if std::env::args().any(|arg| arg == "--init") {
        store.update(&[])?
    } else {
        store.read()?
    };
    println!("{}", store.path().display());
    println!("{}", serde_json::to_string_pretty(&document.0)?);
    Ok(())
}
