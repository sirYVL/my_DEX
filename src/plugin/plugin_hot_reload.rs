/////////////////////////////////////////////////////////////
// my_DEX/src/plugin/plugin_hot_reload.rs
/////////////////////////////////////////////////////////////


use notify::{Watcher, RecursiveMode, watcher, DebouncedEvent};
use std::sync::mpsc::channel;
use std::time::Duration;
use std::path::PathBuf;

/// Beobachtet ein Plugin-Verzeichnis und ruft den Callback `on_change` auf, wenn �nderungen erkannt werden.
/// Dies erm�glicht ein Hot-Reloading von Plugins in einer dezentralen Umgebung.
pub fn watch_plugin_directory<F>(plugin_dir: PathBuf, mut on_change: F) -> notify::Result<()>
where
    F: FnMut(PathBuf) + Send + 'static,
{
    let (tx, rx) = channel();
    let mut watcher = watcher(tx, Duration::from_secs(2))?;
    watcher.watch(&plugin_dir, RecursiveMode::Recursive)?;

    std::thread::spawn(move || {
        loop {
            match rx.recv() {
                Ok(event) => match event {
                    DebouncedEvent::Create(path)
                    | DebouncedEvent::Write(path)
                    | DebouncedEvent::Remove(path)
                    | DebouncedEvent::Rename(_, path) => {
                        on_change(path);
                    },
                    _ => {}
                },
                Err(e) => println!("Beobachtungsfehler: {:?}", e),
            }
        }
    });

    Ok(())
}
