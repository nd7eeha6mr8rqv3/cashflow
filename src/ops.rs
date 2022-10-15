use rust_decimal::Decimal;

use crate::{
    errors::Error,
    types::{
        Account, AccountBook, ClientId, MemoryAccountBook, MemoryTransactionLog, TransactionLog,
        TransactionState, TransactionType, DECIMAL_SCALE,
    },
};
impl Account {
    /// Adds funds to an account's available funds.
    /// # Errors
    /// [`Error::Locked`] if the account is locked
    fn deposit(&mut self, mut amount: Decimal) -> Result<(), Error> {
        self.check_lock()?;
        amount.rescale(DECIMAL_SCALE);
        self.funds_available += amount;
        Ok(())
    }

    /// Subtracts funds from an account's available funds.
    ///
    /// Account balances are allowed to go negative, if the amount
    /// exceeds the available funds.
    /// # Errors
    /// [`Error::Locked`] if the account is locked
    fn withdraw(&mut self, mut amount: Decimal) -> Result<(), Error> {
        self.check_lock()?;
        amount.rescale(DECIMAL_SCALE);
        self.funds_available -= amount;
        Ok(())
    }

    /// Moves funds out of available to held funds.
    ///
    /// Account balances are allowed to go negative, if the amount
    /// exceeds the available funds.
    ///
    /// This operation will succeed on locked accounts.
    fn dispute(&mut self, mut amount: Decimal) {
        amount.rescale(DECIMAL_SCALE);
        self.funds_available -= amount;
        self.funds_held += amount;
    }

    /// Moves funds out of held funds into available funds.
    ///
    /// Account balances are allowed to go negative, if the amount
    /// exceeds the held funds.
    fn resolve(&mut self, mut amount: Decimal) {
        amount.rescale(DECIMAL_SCALE);
        self.funds_held -= amount;
        self.funds_available += amount;
    }

    /// Subtracts funds from held funds and locks the account.
    ///
    /// Account balances are allowed to go negative, if the amount
    /// exceeds the held funds.
    fn chargeback(&mut self, mut amount: Decimal) {
        amount.rescale(DECIMAL_SCALE);
        self.funds_held -= amount;
        self.locked = true;
    }

    /// Returns an [`Error::Locked`] if the account is locked.
    fn check_lock(&self) -> Result<(), Error> {
        if self.locked {
            return Err(Error::Locked(self.client_id));
        }
        Ok(())
    }
}

/// Does the work of applying each incoming transaction to the account book, and storing it in the log.
///
/// This takes a [`TransactionState`] to ensure that each transaction is only applied once.
pub(crate) fn apply_transaction<A, T>(
    account_book: &mut A,
    transaction_log: &mut T,
    transaction_state: &mut TransactionState,
) -> Result<(), Error>
where
    A: AccountBook,
    for<'a> &'a A: IntoIterator<Item = &'a Account>,
    T: TransactionLog,
{
    match transaction_state {
        // Error for already-applied transactions
        TransactionState::Applied(txn_id) => return Err(Error::Duplicate(*txn_id)),
        TransactionState::NotApplied(transaction) => {
            let transaction_id = transaction.transaction_id;
            let referred_amount = transaction_log
                .transaction(transaction_id)?
                .and_then(|referred| referred.amount);
            let account = account_book.account_mut(transaction.client_id)?;
            match transaction.transaction_type {
                TransactionType::Deposit => account.deposit(transaction.amount.unwrap())?,
                TransactionType::Withdrawal => account.withdraw(transaction.amount.unwrap())?,
                // Ignoring missing referred transactions (or referred transactions with no amounts)
                // for the operations below
                TransactionType::Dispute => {
                    if let Some(amount) = referred_amount {
                        account.dispute(amount)
                    }
                }
                TransactionType::Resolve => {
                    if let Some(amount) = referred_amount {
                        account.resolve(amount)
                    }
                }
                TransactionType::Chargeback => {
                    if let Some(amount) = referred_amount {
                        account.chargeback(amount)
                    }
                }
            }
            // Since the input was a mutable reference to an enum, we can swap it out for a new
            // [`TransactionState::Applied`], allowing us to move the input `Transaction` to the
            // internal storage.
            let mut new_state = TransactionState::Applied(transaction.transaction_id);
            std::mem::swap(transaction_state, &mut new_state);
            match new_state {
                TransactionState::NotApplied(txn) => match txn.transaction_type {
                    // Deposits and withdrawals get added to the transaction register, for future reference
                    TransactionType::Deposit | TransactionType::Withdrawal => {
                        transaction_log.register(txn)?;
                    }
                    _ => (),
                },
                TransactionState::Applied(_) => unreachable!(),
            }
        }
    }
    Ok(())
}

impl AccountBook for MemoryAccountBook {
    fn account(&mut self, client_id: ClientId) -> Result<&Account, Error> {
        Ok(self
            .accounts
            .entry(client_id)
            .or_insert_with(|| Account::new(client_id)))
    }

    fn account_mut(&mut self, client_id: ClientId) -> Result<&mut Account, Error> {
        Ok(self
            .accounts
            .entry(client_id)
            .or_insert_with(|| Account::new(client_id)))
    }
}

impl<'a> IntoIterator for &'a MemoryAccountBook {
    type Item = &'a Account;

    type IntoIter = std::collections::hash_map::Values<'a, ClientId, Account>;

    fn into_iter(self) -> Self::IntoIter {
        self.accounts.values()
    }
}

impl IntoIterator for MemoryAccountBook {
    type Item = Account;
    type IntoIter = std::collections::hash_map::IntoValues<ClientId, Account>;

    fn into_iter(self) -> Self::IntoIter {
        self.accounts.into_values()
    }
}

impl TransactionLog for MemoryTransactionLog {
    fn transaction(
        &self,
        transaction_id: crate::types::TransactionId,
    ) -> Result<Option<&crate::types::Transaction>, Error> {
        Ok(self.transactions.get(&transaction_id))
    }

    fn register(&mut self, transaction: crate::types::Transaction) -> Result<(), Error> {
        self.transactions
            .insert(transaction.transaction_id, transaction);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use crate::types::{Transaction, TransactionId};

    use super::*;

    #[test]
    fn test_deposit() {
        let mut account = Account::new(44.into());
        account.deposit(dec!(4.35)).unwrap();
        assert_eq!(account.funds_available(), dec!(4.35));
        account.deposit(dec!(2.47724244)).unwrap();
        assert_eq!(account.funds_available(), dec!(6.8272));
        assert_eq!(account.funds_held(), dec!(0));
    }

    #[test]
    fn test_withdrawal() {
        let mut account = Account::new(35.into());
        account.deposit(dec!(44.865)).unwrap();
        account.withdraw(dec!(2.47724244)).unwrap();
        assert_eq!(account.funds_available(), dec!(42.3878));
        assert_eq!(account.funds_held(), dec!(0));
    }

    #[test]
    fn test_dispute_and_resolve() {
        let mut account = Account::new(26.into());
        account.deposit(dec!(2.8422)).unwrap();
        account.dispute(dec!(2.8422));
        assert_eq!(account.funds_available(), dec!(0));
        assert_eq!(account.funds_held(), dec!(2.8422));
        account.resolve(dec!(2.8422));
        assert_eq!(account.funds_available(), dec!(2.8422));
        assert_eq!(account.funds_held(), dec!(0));
    }

    #[test]
    fn test_chargeback_and_lock() {
        let mut account = Account::new(24.into());
        account.deposit(dec!(4.652)).unwrap();
        account.dispute(dec!(4.652));
        assert_eq!(account.funds_held(), dec!(4.652));
        account.chargeback(dec!(4.652));
        assert_eq!(account.funds_held(), dec!(0));
        assert!(account.is_locked());
        assert!(account.deposit(dec!(2.00)).is_err());
    }

    #[test]
    fn test_create_account_on_demand() {
        let mut book = MemoryAccountBook::new();
        let account = book.account(24.into()).unwrap();
        assert_eq!(account.client_id, ClientId::from(24));
    }

    #[test]
    fn test_persist_account() {
        let mut book = MemoryAccountBook::new();
        let account = book.account_mut(25.into()).unwrap();
        assert_eq!(account.client_id, ClientId::from(25));
        account.deposit(dec!(4.4444)).unwrap();
        let account = book.account_mut(25.into()).unwrap();
        assert_eq!(account.funds_available(), dec!(4.4444));
    }

    #[test]
    fn test_apply_deposit() {
        let mut accounts = MemoryAccountBook::new();
        let mut txnlog = MemoryTransactionLog::new();
        let transaction = Transaction {
            transaction_type: TransactionType::Deposit,
            client_id: ClientId::from(41),
            transaction_id: TransactionId::from(3311),
            amount: Some(dec!(24.22)),
        };
        let mut state = transaction.into();
        apply_transaction(&mut accounts, &mut txnlog, &mut state).unwrap();
        match state {
            TransactionState::Applied(txnid) => assert_eq!(txnid, TransactionId::from(3311)),
            TransactionState::NotApplied(_) => panic!("Transaction was not applied"),
        }
        let account = accounts.account_mut(41.into()).unwrap();
        assert_eq!(account.funds_available(), dec!(24.22));
    }

    #[test]
    fn test_apply_series() {
        let mut accounts = MemoryAccountBook::new();
        let mut txnlog = MemoryTransactionLog::new();
        let transaction = Transaction {
            transaction_type: TransactionType::Deposit,
            client_id: ClientId::from(41),
            transaction_id: TransactionId::from(3311),
            amount: Some(dec!(24.22)),
        };
        apply_transaction(&mut accounts, &mut txnlog, &mut transaction.into()).unwrap();
        let transaction = Transaction {
            transaction_type: TransactionType::Withdrawal,
            client_id: ClientId::from(41),
            transaction_id: TransactionId::from(3312),
            amount: Some(dec!(0.21)),
        };
        apply_transaction(&mut accounts, &mut txnlog, &mut transaction.into()).unwrap();
        let transaction = Transaction {
            transaction_type: TransactionType::Deposit,
            client_id: ClientId::from(41),
            transaction_id: TransactionId::from(3313),
            amount: Some(dec!(7.8484)),
        };
        apply_transaction(&mut accounts, &mut txnlog, &mut transaction.into()).unwrap();
        let transaction = Transaction {
            transaction_type: TransactionType::Dispute,
            client_id: ClientId::from(41),
            transaction_id: TransactionId::from(3313),
            amount: None,
        };
        apply_transaction(&mut accounts, &mut txnlog, &mut transaction.into()).unwrap();
        let account = accounts.account_mut(41.into()).unwrap();
        assert_eq!(account.funds_available(), dec!(24.01));
        assert_eq!(account.funds_held(), dec!(7.8484));
        // Missing txnid below
        let transaction = Transaction {
            transaction_type: TransactionType::Dispute,
            client_id: ClientId::from(41),
            transaction_id: TransactionId::from(3319),
            amount: None,
        };
        apply_transaction(&mut accounts, &mut txnlog, &mut transaction.into()).unwrap();
        let account = accounts.account_mut(41.into()).unwrap();
        assert_eq!(account.funds_available(), dec!(24.01));
        assert_eq!(account.funds_held(), dec!(7.8484));
        let transaction = Transaction {
            transaction_type: TransactionType::Resolve,
            client_id: ClientId::from(41),
            transaction_id: TransactionId::from(3313),
            amount: None,
        };
        apply_transaction(&mut accounts, &mut txnlog, &mut transaction.into()).unwrap();
        let account = accounts.account_mut(41.into()).unwrap();
        assert_eq!(account.funds_available(), dec!(31.8584));
        assert_eq!(account.funds_held(), dec!(0));
        let transaction = Transaction {
            transaction_type: TransactionType::Dispute,
            client_id: ClientId::from(41),
            transaction_id: TransactionId::from(3313),
            amount: None,
        };
        apply_transaction(&mut accounts, &mut txnlog, &mut transaction.into()).unwrap();
        let transaction = Transaction {
            transaction_type: TransactionType::Chargeback,
            client_id: ClientId::from(41),
            transaction_id: TransactionId::from(3313),
            amount: None,
        };
        apply_transaction(&mut accounts, &mut txnlog, &mut transaction.into()).unwrap();
        let account = accounts.account_mut(41.into()).unwrap();
        assert_eq!(account.funds_available(), dec!(24.01));
        assert_eq!(account.funds_held(), dec!(0));
        let transaction = Transaction {
            transaction_type: TransactionType::Deposit,
            client_id: ClientId::from(41),
            transaction_id: TransactionId::from(3314),
            amount: Some(dec!(17.4219)),
        };
        let mut state = transaction.into();
        assert!(apply_transaction(&mut accounts, &mut txnlog, &mut state).is_err());
        match state {
            TransactionState::Applied(_) => panic!("Transaction was erroneously applied"),
            TransactionState::NotApplied(txn) => {
                assert_eq!(txn.transaction_id, TransactionId::from(3314))
            }
        }
    }
}
