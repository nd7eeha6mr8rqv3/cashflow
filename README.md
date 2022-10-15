# Cashflow - processing your transactions since late 2022

This is a barebones transaction engine that can handle certain account operations streaming in in a transaction
log, and report account state at any point.

It's designed to be used as a crate in your own codebase, and can be extended to remote storage or whatever by
implementing a couple of traits. Currently, the crate provides basic in-memory implementations of traits.

IO implementations so far support CSV reading and writing.

## Running
Included is a command-line tool that can read a single CSV file containing transactions.
### Example
```bash
cargo run -- transactions.csv > accounts.csv
```

Or, if you don't have any Rust tools installed, you can run the command line tool from Docker:
```bash
docker build -t cashflow:latest .
docker run -it --rm -v $(PWD):/working cashflow:latest transactions.csv > accounts.csv
```

## Using in code
The command line implementation is a decent introduction. Basically, you'll need a [`AccountBook`](crate::types::AccountBook) to hold accounts,
and a [`TransactionLog`](crate::types::TransactionLog) to keep track of transactions. Then use functions in [`io`] to load them and output
the updated account register.
### Example
```rust
    # use cashflow::{io, types::{MemoryAccountBook, MemoryTransactionLog}};
    # use std::{fs::File, io::BufReader, io::Cursor};
    // Here's where you might want to read a file or whatnot
    let mut log_reader = Cursor::new(vec![]);
    let mut account_book = MemoryAccountBook::new();
    let mut transaction_log = MemoryTransactionLog::new();
    io::load_transactions_from_csv(&mut log_reader, &mut account_book, &mut transaction_log).unwrap();
    // Printing accounts to stdout
    let mut stdout = std::io::stdout().lock();
    io::write_accounts_to_csv(&mut stdout, &account_book).unwrap()
```

## Design choices that might spark questions
In the interest of time and simplicity, there are a few significant limitations:
 - No bounds checking on account values. Transactions will allow, for example, withdrawals on a zero balance. The result will be negative balances. Zero amounts aren't treated specially; a zero amount chargeback will still lock the account, etc.
 - No true transaction log. Transactions are stored in a [`HashMap`](std::collections::HashMap) by their ID, and only for the purpose of referring back to them in the case of [`TransactionType::Dispute`](types::TransactionType::Dispute), [`TransactionType::Resolve`](types::TransactionType::Resolve), and
 [`TransactionType::Chargeback`](types::TransactionType::Chargeback) types.
 - Incoming duplicate transactions will be re-applied without errors. A transaction can be disputed multiple times, resolved before dispute. If a withdrawal and a deposit share the same transaction ID, the newer transaction will completely replace the older one. This mainly impacts any future operations that refer back to this transaction by ID.
 - Disputes and resolutions and chargebacks are strange, because disputing a withdrawal or a deposit will both move funds into held funds, regardless of which type of transaction is being disputed.
 - No check is done to ensure client IDs and transactions agree for referring transactions.
 - In general, this is heavily geared towards generating a correct final account report from an incoming list of transactions, assuming no errors in the input data. There's not much in the way of queryable account history
 - I tried to make [`AccountBook`](types::AccountBook) allow iteration over its accounts, composable with adapters, but trying to make an iterable trait that didn't consume `self` was beyond me given the time limitations. Or, actually that worked ok using higher-ranked trait bounds, but I didn't work out a function signature in the [`io`] functions that was compatible.
 - [`Account`](types::Account) would also likely make sense as a trait, to allow eg RPC calls to update account information in another system.
 - No tracing or logging, mainly because we're already using stdout for output, and I didn't want to wrangle that stuff beyond the defaults.
 - Profiling shows that 92% of CPU time is spent reading from CSV. Which makes sense; this isn't doing complex math. But, that's the place to put in some work if we want this to run faster.

Some choices are also unusual, given the codebase size or expected usage:
 - Implementing traits for [`AccountBook`](types::AccountBook) and [`TransactionLog`](types::TransactionLog) to enable pluggable backends. Adds a lot of complexity relative to codebase size, and introduces the iteration limitation as mentioned above, but it shows how this might work in a larger project. (And hopefully isn't too confusing for anyone using this.)
 - Error types are enumerated, rather than using `anyhow::Error` or similar. This may generalize a bit as this being designed primarily as a library crate, not as an application. Rich error types are nice coming from crates, as it can enable thoughtful retry behavior or other graceful error handling.
 - The whole [`TransactionState`](types::TransactionState) thing is weird. See documentation for commentary.