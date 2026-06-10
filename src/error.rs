//! Error type mirroring libSetReplace's error conditions.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Initial-state atoms must be positive integers (libSetReplace
    /// `Error::NonPositiveAtoms`).
    NonPositiveAtoms,
    /// The inputs of a rule do not form a connected hypergraph: while
    /// completing a match, every remaining input edge consisted entirely of
    /// unbound pattern variables. libSetReplace does not support such rules
    /// (`HypergraphMatcher::Error::DisconnectedInputs`); Wolfram Language
    /// falls back to a slow symbolic method for them.
    DisconnectedInputs,
    /// Ran out of atom names (libSetReplace `Error::AtomCountOverflow`).
    AtomCountOverflow,
    /// Rule failed validation (e.g. no input edges, zero atoms).
    InvalidRule(String),
    /// Rule/state parse error.
    Parse(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NonPositiveAtoms => {
                write!(f, "initial-state atoms must be positive integers")
            }
            Error::DisconnectedInputs => write!(
                f,
                "rule inputs do not form a connected hypergraph (not supported)"
            ),
            Error::AtomCountOverflow => write!(f, "ran out of atom names"),
            Error::InvalidRule(msg) => write!(f, "invalid rule: {msg}"),
            Error::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}
