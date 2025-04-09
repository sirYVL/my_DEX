// src/error.rs
//
// "Reale" Error-Definitionen, ohne Demo-Charakter.
// Nutzt thiserror, differenziert kritische und recoverable Fehler.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DexError {
    // Fatal: DB kaputt => Node kann nicht fortfahren
    #[error("Database error: {0}")]
    DatabaseError(String),

    // Recoverable: z.B. unbekannter Peer => wir können netzwerkseitig ignorieren
    #[error("Peer not found: {peer_id}")]
    PeerNotFound { peer_id: String },

    // Recoverable: Order existiert nicht
    #[error("Order {order_id} not found")]
    OrderNotFound { order_id: String },

    // Net-Partition => wir können im Offline-Mode weitermachen
    #[error("Network partition encountered")]
    NetworkPartition,

    // Zeitüberschreitung bei Atomic-Swap => wir können refund
    #[error("Atomic swap timed out")]
    SwapTimeout,

    // Teil-Füllung schlägt fehl
    #[error("Partial fill on {order_id} is invalid => {reason}")]
    PartialFillError {
        order_id: String,
        reason: String,
    },

    // Account existiert bereits
    #[error("Account {0} already exists")]
    AccountAlreadyExists(String),

    // Account nicht gefunden
    #[error("Account {0} not found")]
    AccountNotFound(String),

    // Wallet existiert bereits
    #[error("Wallet {0} already exists")]
    WalletAlreadyExists(String),

    // Wallet nicht gefunden
    #[error("Wallet {0} not found")]
    WalletNotFound(String),

    #[error("Cannot delete account {0}: balances not empty")]
    CannotDeleteNonEmptyAccount(String),

    #[error("Account {0} is paused and cannot perform new trades")]
    AccountIsPaused(String),

    // Sammel-Fehler
    #[error("Other error: {0}")]
    Other(String),
}
