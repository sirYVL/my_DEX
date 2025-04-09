/////////////////////////////////////////////////////////////
// my_DEX/src/plugin/plugin_loader.rs
/////////////////////////////////////////////////////////////

use libloading::{Library, Symbol};
use std::path::Path;
use crate::plugin::plugin_api::Plugin;

/// L�dt dynamisch Plugins aus shared libraries und beh�lt diese im Speicher.
pub struct PluginLoader {
    pub libraries: Vec<Library>,
}

impl PluginLoader {
    /// Erzeugt einen neuen PluginLoader.
    pub fn new() -> Self {
        PluginLoader {
            libraries: Vec::new(),
        }
    }
    
    /// L�dt ein Plugin aus der angegebenen Bibliothek.
    /// Erwartet, dass in der Library ein Symbol `plugin_create` existiert, das ein Pointer auf ein `dyn Plugin` zur�ckgibt.
    pub unsafe fn load_plugin<P: AsRef<Path>>(&mut self, lib_path: P) -> Result<Box<dyn Plugin>, Box<dyn std::error::Error>> {
        let lib = Library::new(lib_path.as_ref())?;
        // Die Bibliothek wird in der internen Liste gespeichert, um ein Entladen zu verhindern.
        self.libraries.push(lib);
        let lib_ref = self.libraries.last().unwrap();
        // Erwarte das Symbol "plugin_create"
        let func: Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> = lib_ref.get(b"plugin_create")?;
        let boxed_raw = func();
        // Konvertiere den rohen Pointer in ein Box
        let boxed_plugin = Box::from_raw(boxed_raw);
        Ok(boxed_plugin)
    }
}
