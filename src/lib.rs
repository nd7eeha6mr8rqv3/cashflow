#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
/// Error handling and custom [`Error`](std::error::Error) types
pub mod errors;
/// Functions for reading and writing transaction logs and account states
pub mod io;
/// Business logic for processing transactions
mod ops;
/// Data types used throughout Cashflow
pub mod types;
