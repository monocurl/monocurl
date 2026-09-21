//! Hand-written reference operators, implemented directly against the raw
//! `OperatorNode`/`Keyed` traits (no macros). `shift` is milestone 1's
//! reference operator per `OPERATORS.md`.

mod shift;

pub use shift::{Shift, ShiftChainExt};
