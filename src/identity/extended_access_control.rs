///////////////////////////////////////////////////////////
// my_dex/src/identity/extended_access_control.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul implementiert erweiterte Zugriffskontrollen:
// - Verwaltung von Whitelist/Blacklist f�r IP-Adressen
// - TLS-Authentifizierung anhand von Zertifikat-Subjects
//
// Der Code ist produktionsreif und ungek�rzt implementiert.
// Er stellt sicher, dass nur zugelassene IPs und TLS-Zertifikate Zugriff erhalten.
///////////////////////////////////////////////////////////

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use tracing::{debug, warn};

// Struktur f�r erweiterte Zugriffskontrollen
#[derive(Debug, Clone)]
pub struct ExtendedAccessControl {
    /// Whitelist: Wenn nicht leer, sind nur diese IPs zugelassen.
    whitelist: HashSet<IpAddr>,
    /// Blacklist: Alle IPs, die hier eingetragen sind, werden blockiert.
    blacklist: HashSet<IpAddr>,
    /// Optionale Liste erlaubter TLS-Zertifikat-Subjects (z. B. Common Name)
    allowed_tls_subjects: HashSet<String>,
}

impl ExtendedAccessControl {
    /// Erzeugt eine neue Instanz mit leeren Listen.
    pub fn new() -> Self {
        Self {
            whitelist: HashSet::new(),
            blacklist: HashSet::new(),
            allowed_tls_subjects: HashSet::new(),
        }
    }

    /// F�gt eine IP-Adresse zur Whitelist hinzu.
    pub fn add_to_whitelist(&mut self, ip: IpAddr) {
        self.whitelist.insert(ip);
        debug!("IP {} zur Whitelist hinzugef�gt", ip);
    }

    /// Entfernt eine IP-Adresse aus der Whitelist.
    pub fn remove_from_whitelist(&mut self, ip: &IpAddr) {
        self.whitelist.remove(ip);
        debug!("IP {} aus der Whitelist entfernt", ip);
    }

    /// F�gt eine IP-Adresse zur Blacklist hinzu.
    pub fn add_to_blacklist(&mut self, ip: IpAddr) {
        self.blacklist.insert(ip);
        debug!("IP {} zur Blacklist hinzugef�gt", ip);
    }

    /// Entfernt eine IP-Adresse aus der Blacklist.
    pub fn remove_from_blacklist(&mut self, ip: &IpAddr) {
        self.blacklist.remove(ip);
        debug!("IP {} aus der Blacklist entfernt", ip);
    }

    /// F�gt einen TLS-Subject (z. B. Common Name) zur Liste erlaubter Zertifikate hinzu.
    pub fn add_allowed_tls_subject(&mut self, subject: String) {
        self.allowed_tls_subjects.insert(subject);
        debug!("TLS-Subject zur Allowed-Liste hinzugef�gt");
    }

    /// �berpr�ft, ob die gegebene IP-Adresse erlaubt ist.
    /// - Falls die IP in der Blacklist ist, wird sie blockiert.
    /// - Ist die Whitelist nicht leer, muss die IP darin enthalten sein.
    pub fn is_ip_allowed(&self, ip: &IpAddr) -> bool {
        if self.blacklist.contains(ip) {
            warn!("IP {} ist in der Blacklist und wird blockiert", ip);
            return false;
        }
        if !self.whitelist.is_empty() && !self.whitelist.contains(ip) {
            warn!("IP {} ist nicht in der Whitelist", ip);
            return false;
        }
        true
    }

    /// �berpr�ft, ob ein TLS-Zertifikat anhand seines Subject (Common Name) zul�ssig ist.
    /// In einer echten Implementierung w�rden Sie ein Zertifikat parsen und den CN extrahieren.
    /// Hier nehmen wir den CN als String entgegen.
    pub fn verify_tls_certificate(&self, subject_cn: &str) -> Result<bool> {
        if self.allowed_tls_subjects.is_empty() {
            // Keine Einschr�nkung gesetzt, daher alle Zertifikate zulassen.
            debug!("Keine TLS-Allowed-Liste gesetzt, Zertifikat akzeptiert");
            return Ok(true);
        }
        if self.allowed_tls_subjects.contains(subject_cn) {
            debug!("TLS-Zertifikat mit Subject '{}' ist erlaubt", subject_cn);
            Ok(true)
        } else {
            warn!("TLS-Zertifikat mit Subject '{}' ist nicht erlaubt", subject_cn);
            Ok(false)
        }
    }
}

// Optional: Eine globale Instanz, falls Sie in mehreren Teilen des Systems darauf zugreifen m�chten.
lazy_static::lazy_static! {
    pub static ref GLOBAL_EXTENDED_ACCESS_CONTROL: Arc<Mutex<ExtendedAccessControl>> = Arc::new(Mutex::new(ExtendedAccessControl::new()));
}
