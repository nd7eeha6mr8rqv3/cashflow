use cashflow::io;
use cashflow::types::{MemoryAccountBook, MemoryTransactionLog};
use std::{fs::File, io::BufReader};

fn main() {
    let log_filename = std::env::args()
        .nth(1)
        .expect("Usage: cashflow {transactions.csv}");
    let log_file = File::open(&log_filename)
        .unwrap_or_else(|err| panic!("Couldn't open transaction log at {log_filename}: {err}"));
    let mut log_reader = BufReader::new(log_file);
    let mut account_book = MemoryAccountBook::new();
    let mut transaction_log = MemoryTransactionLog::new();
    io::load_transactions_from_csv(&mut log_reader, &mut account_book, &mut transaction_log)
        .unwrap_or_else(|err| panic!("Failed to load transactions from CSV file: {err}"));
    let mut stdout = std::io::stdout().lock();
    io::write_accounts_to_csv(&mut stdout, &account_book)
        .unwrap_or_else(|err| panic!("Failed to write accounts to CSV: {err}"));
}
