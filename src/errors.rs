use crate::types::{ClientId, TransactionId};

/// Error type that can be returned by fallible operations in this crate
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error reading or writing CSV files; could wrap IO or parsing errors
    #[error("Error processing CSV")]
    Load(#[from] csv::Error),
    /// Once a [`Transaction`](crate::types::Transaction) has been successfully applied, it cannot be applied again.
    /// If that happens, this error will be returned.
    /// Note that duplicate transactions in the incoming stream will each be applied without causing a duplicate error.
    #[error("Attempt to re-apply already applied transaction id {0}")]
    Duplicate(TransactionId),
    /// If an account is locked, and the operation is not allowed on locked accounts, this error will be returned
    #[error("Account {0} is locked")]
    Locked(ClientId),
}
