//! Common datatypes supporting functions throughout the Cashflow Engine

use std::{collections::HashMap, fmt::Display};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{errors::Error, ops};

/// The number of decimals to track for all amounts
pub const DECIMAL_SCALE: u32 = 4;

/// Unique identifier for a client
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(u16);

impl From<u16> for ClientId {
    fn from(client_id: u16) -> Self {
        Self(client_id)
    }
}

impl Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "id[{}]", self.0)
    }
}

/// Unique identifier for a transaction
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionId(u32);

impl From<u32> for TransactionId {
    fn from(transaction_id: u32) -> Self {
        Self(transaction_id)
    }
}

impl Display for TransactionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "id[{}]", self.0)
    }
}

/// Represents the different types of operations that can be performed on a client's account
#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    /// Credit to the client's asset account
    Deposit,
    /// Debit to the client's asset account
    Withdrawal,
    /// Represents a client's claim that a transaction was erroneous and should be reversed
    Dispute,
    /// Represents a resolution to a disput, releasing the associated held funds
    Resolve,
    /// Final state of a dispute and represents a client reversing a transaction
    Chargeback,
}

/// A holder for an incoming [`Transaction`] that ensures it can only be applied once.
///
/// This mainly exists because we aren't allowing [`Clone`] for [`Transaction`]s, since
/// inadvertent duplication would be so disastrous. Since the [`TransactionLog`] wants ownership
/// of the [`Transaction`], this is a way to take ownership if the transaction applies successfully,
/// but to allow the caller to retain it if it doesn't. (Then, the caller can retry or whatever they
/// wish to do.)
#[derive(Debug)]
pub enum TransactionState {
    /// Contains a [`Transaction`] that has not yet been applied successfully.
    NotApplied(Transaction),
    /// Contains the [`TransactionId`] of a [`Transaction`] that has already been applied.
    Applied(TransactionId),
}

impl From<Transaction> for TransactionState {
    fn from(transaction: Transaction) -> Self {
        Self::NotApplied(transaction)
    }
}

/// Represents an actual operation on a customer's account
#[derive(Debug, Deserialize)]
pub struct Transaction {
    /// The type of this transaction (see [`TransactionType`])
    #[serde(rename = "type")]
    pub(crate) transaction_type: TransactionType,
    /// Account ID for this transaction
    #[serde(rename = "client")]
    pub(crate) client_id: ClientId,
    /// Unique identifier for this transaction
    #[serde(rename = "tx")]
    pub(crate) transaction_id: TransactionId,
    /// The amount of money in this transaction, if applicable.
    /// [`TransactionType::Deposit`] and [`TransactionType::Withdrawal`]
    /// should have amounts.
    #[serde(deserialize_with = "deserialize_option_decimal")]
    pub(crate) amount: Option<Decimal>,
}

/// Function to help [`serde`] deserialize from a string into a [`Decimal`] with [`DECIMAL_SCALE`] scale
fn deserialize_option_decimal<'de, D>(value: D) -> Result<Option<Decimal>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let amount = rust_decimal::serde::str_option::deserialize(value)?;
    if let Some(mut amount) = amount {
        amount.rescale(DECIMAL_SCALE);
    }
    Ok(amount)
}

/// Overall state of a single account held by a client
#[derive(Debug)]
pub struct Account {
    /// The unique identifier for the account
    pub(crate) client_id: ClientId,
    /// The total funds that are available for trading, staking, withdrawal, etc.
    ///
    /// Note that funds may go negative if total withdrawals or disputes are larger than total
    /// deposits.
    pub(crate) funds_available: Decimal,
    /// The total funds that are held for dispute.
    ///
    /// Note that funds may go negative if total resolutions or chargebacks are larger than total
    /// deposits.
    pub(crate) funds_held: Decimal,
    /// Whether the account is locked. An account is locked if a charge back occurs
    pub(crate) locked: bool,
}

impl Account {
    /// Creates a new, empty account, with zero balances
    #[must_use]
    pub fn new(client_id: ClientId) -> Self {
        Self {
            client_id,
            funds_available: Decimal::new(0, DECIMAL_SCALE),
            funds_held: Decimal::new(0, DECIMAL_SCALE),
            locked: false,
        }
    }

    /// Returns the unique identifier for the account
    #[must_use]
    #[inline]
    pub fn client_id(&self) -> ClientId {
        self.client_id
    }

    /// Returns the total funds available
    #[must_use]
    #[inline]
    pub fn funds_available(&self) -> Decimal {
        self.funds_available
    }

    /// Returns the total funds held for dispute
    #[must_use]
    #[inline]
    pub fn funds_held(&self) -> Decimal {
        self.funds_held
    }
    /// Returns total funds in the account, available or held
    #[must_use]
    #[inline]
    pub fn total(&self) -> Decimal {
        self.funds_available + self.funds_held
    }
    /// Returns whether the account is locked
    #[must_use]
    #[inline]
    pub fn is_locked(&self) -> bool {
        self.locked
    }
}

/// An interface to all accounts
pub trait AccountBook: IntoIterator<Item = Account>
where
    for<'a> &'a Self: IntoIterator<Item = &'a Account>,
    Self: Sized,
{
    /// Takes a [`TransactionState`] reference and applies it to an account in the account book
    fn apply<T>(
        &mut self,
        transaction_log: &mut T,
        transaction: &mut TransactionState,
    ) -> Result<(), Error>
    where
        T: TransactionLog,
    {
        ops::apply_transaction(self, transaction_log, transaction)
    }

    /// Fetches a client's account. If an account does not exist yet, it will be created.
    fn account(&mut self, client_id: ClientId) -> Result<&Account, Error>;

    /// Fetches a client's account, returning a mutable reference. If an account does not exist yet,
    /// it will be created.
    fn account_mut(&mut self, client_id: ClientId) -> Result<&mut Account, Error>;
}

/// An interface to all transactions
pub trait TransactionLog {
    /// Fetches a transaction by ID, if one exists
    fn transaction(&self, transaction_id: TransactionId) -> Result<Option<&Transaction>, Error>;

    /// Registers a transaction in the log
    fn register(&mut self, transaction: Transaction) -> Result<(), Error>;
}

/// Holds all accounts in an in-memory structure.
///
/// # Limitations
/// No persistence.
///
/// Only a single operation is allowed on the entire
/// account book at any given time.
#[derive(Default, Debug)]
pub struct MemoryAccountBook {
    /// Storage for the map of account ID to account
    pub(crate) accounts: HashMap<ClientId, Account>,
}

impl MemoryAccountBook {
    /// Creates a new, empty [`MemoryAccountBook`].
    #[must_use]
    pub fn new() -> Self {
        MemoryAccountBook::default()
    }
}

/// Holds all transactions in an in-memory structure.
///
/// # Limitations
/// Only a single transaction per ID is supported,
/// so operations such as [`TransactionType::Dispute`] or
/// [`TransactionType::Resolve`] will not be stored.
///
/// No persistence.
///
/// Only a single operation is allowed on the entire log
/// at any given time.
#[derive(Default, Debug)]
pub struct MemoryTransactionLog {
    /// Storage for transactions that have been registered
    pub(crate) transactions: HashMap<TransactionId, Transaction>,
}

impl MemoryTransactionLog {
    /// Creates a new, empty [`MemoryTransactionLog`]
    #[must_use]
    pub fn new() -> Self {
        MemoryTransactionLog::default()
    }
}
