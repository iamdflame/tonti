//! Tonti actuarial core. `no_std` and float-free so the identical code runs natively (tests,
//! simulator, backtest) and inside the Stylus contract on Robinhood Chain.
#![no_std]

extern crate alloc;

pub mod fixed;
pub mod fraud;
pub mod ledger;
pub mod mortality;
pub mod quote;
pub mod rebalance;
pub mod wide;

pub use fixed::{Fx, MathError, MathResult};
