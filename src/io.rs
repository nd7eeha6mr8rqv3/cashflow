//! Helpers for reading from transaction logs and outputting reports

use std::io::{Read, Write};

use csv::Trim;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::{
    errors::Error,
    types::{Account, AccountBook, ClientId, Transaction, TransactionLog},
};

/// Loads transactions from a CSV-formatted file stream.
///
/// All transactions will be added to the supplied [`TransactionLog`], and will be
/// applied against the supplied [`AccountBook`].
///
/// Expects input data in this format (including header):
/// ```csv
/// type,      client,   tx,   amount
/// deposit,        1,    1,      1.0
/// deposit,        2,    2,      2.0
/// deposit,        1,    3,      2.0
/// withdrawal,     1,    4,      1.5
/// withdrawal,     2,    5,      3.0
/// ```
pub fn load_transactions_from_csv<R, A, T>(
    reader: &mut R,
    account_book: &mut A,
    transaction_log: &mut T,
) -> Result<(), Error>
where
    R: Read,
    A: AccountBook,
    for<'a> &'a A: IntoIterator<Item = &'a Account>,
    T: TransactionLog,
{
    let mut csv_reader = csv::ReaderBuilder::new()
        .trim(Trim::All)
        .flexible(true)
        .from_reader(reader);
    for record in csv_reader.deserialize() {
        let transaction: Transaction = record?;
        account_book.apply(transaction_log, &mut transaction.into())?;
    }
    Ok(())
}

/// Type used for serializing an [`Account`], but also including a `total`.
#[derive(Serialize, Debug)]
struct AccountWithTotal {
    /// The client's unique identifier
    client: ClientId,
    /// The amount of available funds
    available: Decimal,
    /// The amount of held funds
    held: Decimal,
    /// The total amount of funds
    total: Decimal,
    /// Whether the account is locked
    locked: bool,
}

impl From<&Account> for AccountWithTotal {
    fn from(account: &Account) -> Self {
        Self {
            client: account.client_id(),
            available: account.funds_available(),
            held: account.funds_held(),
            total: account.total(),
            locked: account.is_locked(),
        }
    }
}

/// Outputs the state of the supplied accounts to CSV.
///
/// See [`Account`] for more details on the meaning of each field.
///
/// Output data will be in the form:
/// ```csv
/// client,available,held,total,locked
/// 2,2,0,2,false
/// 1,1.5,0,1.5,false
/// ```
pub fn write_accounts_to_csv<W, A>(writer: &mut W, account_book: &A) -> Result<(), Error>
where
    W: Write,
    for<'a> &'a A: IntoIterator<Item = &'a Account>,
{
    let mut csv_writer = csv::Writer::from_writer(writer);
    for account in account_book {
        csv_writer.serialize(AccountWithTotal::from(account))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use csv::StringRecord;
    use rust_decimal_macros::dec;

    use crate::types::{MemoryAccountBook, MemoryTransactionLog};

    use super::*;

    const TEST_INPUT_CSV: &[u8] = b"type,      client,   tx,   amount
deposit,        1,    1,      7.0
deposit,        2,    2,      2.0
deposit,        1,    3,      2.0
withdrawal,     1,    4,      1.5
withdrawal,     2,    5,      3.0
deposit,        2,    6,      2.0
dispute,        2,    2,
resolve,        2,    2
";

    #[test]
    fn test_read_with_whitespace_and_missing_commas() {
        let mut book = MemoryAccountBook::new();
        let mut txnlog = MemoryTransactionLog::new();
        let mut cursor = Cursor::new(TEST_INPUT_CSV);
        load_transactions_from_csv(&mut cursor, &mut book, &mut txnlog).unwrap();
        assert_eq!(book.account(1.into()).unwrap().funds_available(), dec!(7.5));
        assert_eq!(book.account(2.into()).unwrap().funds_available(), dec!(1));
    }

    #[test]
    fn test_write_with_whitespace_and_missing_commas() {
        let mut book = MemoryAccountBook::new();
        let mut txnlog = MemoryTransactionLog::new();
        let mut cursor = Cursor::new(TEST_INPUT_CSV);
        load_transactions_from_csv(&mut cursor, &mut book, &mut txnlog).unwrap();
        let mut output = vec![];
        write_accounts_to_csv(&mut output, &book).unwrap();

        // These contortions above are all because there's no guarantee of client ID output order.
        let mut csv_reader = csv::Reader::from_reader(Cursor::new(&output));
        let mut record = StringRecord::new();
        csv_reader.read_record(&mut record).unwrap();
        match record.get(0).unwrap() {
            "1" => assert_eq!(record.get(1), Some("7.5000")),
            "2" => assert_eq!(record.get(1), Some("1.0000")),
            _ => panic!("Unexpected output in record"),
        }
        csv_reader.read_record(&mut record).unwrap();
        match record.get(0).unwrap() {
            "1" => assert_eq!(record.get(1), Some("7.5000")),
            "2" => assert_eq!(record.get(1), Some("1.0000")),
            _ => panic!("Unexpected output in record"),
        }
    }
}
