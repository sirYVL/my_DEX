//////////////////////////////////////////////////////
/// my_DEX/src/plugin/plugin_api.rs
//////////////////////////////////////////////////////


/// Das zentrale Plugin-Interface, das von allen externen Plugins implementiert werden muss.
pub trait Plugin {
    /// Gibt den Namen des Plugins zur�ck.
    fn name(&self) -> &'static str;
    /// F�hrt die Hauptfunktionalit�t des Plugins aus.
    fn execute(&self) -> Result<(), Box<dyn std::error::Error>>;
}
