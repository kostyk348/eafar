//! Error type for the whole crate.
//!
//! C++ lineage: the original library reports failures by throwing
//! `std::out_of_range`, `std::invalid_argument` and `std::runtime_error`.
//! The Rust port turns every throw site into a [`Result`] value so callers
//! handle errors explicitly; the variants mirror the exception kinds.

use std::fmt;

/// Errors produced by eafar operations (C++ exceptions, ported as values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A coordinate, index or region id was outside the valid range
    /// (C++ `std::out_of_range` on spatial access).
    OutOfBounds(String),
    /// An argument value was invalid for the operation
    /// (C++ `std::invalid_argument`).
    InvalidArgument(String),
    /// A lookup key (membership name, automaton name) was not present
    /// (C++ `std::out_of_range` on map-like lookups).
    NotFound(String),
    /// A file could not be written during visualization export
    /// (C++ `std::runtime_error`).
    Io(String),
}

impl Error {
    /// Shorthand constructor for [`Error::OutOfBounds`].
    pub fn out_of_bounds(msg: impl Into<String>) -> Self {
        Error::OutOfBounds(msg.into())
    }

    /// Shorthand constructor for [`Error::InvalidArgument`].
    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Error::InvalidArgument(msg.into())
    }

    /// Shorthand constructor for [`Error::NotFound`].
    pub fn not_found(msg: impl Into<String>) -> Self {
        Error::NotFound(msg.into())
    }

    /// Shorthand constructor for [`Error::Io`].
    pub fn io(msg: impl Into<String>) -> Self {
        Error::Io(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::OutOfBounds(msg) => write!(f, "out of bounds: {msg}"),
            Error::InvalidArgument(msg) => write!(f, "invalid argument: {msg}"),
            Error::NotFound(msg) => write!(f, "not found: {msg}"),
            Error::Io(msg) => write!(f, "io error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

/// Crate-wide result type (single error type, like the C++ exception set).
pub type Result<T> = std::result::Result<T, Error>;
