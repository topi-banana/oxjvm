//! Runtime errors.

use alloc::string::String;
use core::fmt;

use crate::value::ObjectRef;

/// Everything that can stop the interpreter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    /// A Java exception is propagating; the payload is the throwable object.
    Thrown(ObjectRef),
    /// The guest called `System.exit` (or another host-terminating operation).
    Exit(i32),
    /// A host operation failed.
    Host(String),
    /// The class file or bytecode is invalid; reported with the class and method when known.
    InvalidCode {
        /// The class or method being verified.
        context: String,
        /// What was wrong.
        message: String,
    },
    /// The runtime itself found an inconsistent state. A correctness bug in the VM.
    Internal(String),
}

impl VmError {
    /// Build an `InvalidCode` error.
    pub fn invalid_code(context: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidCode {
            context: context.into(),
            message: message.into(),
        }
    }

    /// Build an `Internal` error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// Whether this is a thrown Java exception.
    #[must_use]
    pub const fn is_thrown(&self) -> bool {
        matches!(self, Self::Thrown(_))
    }
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Thrown(reference) => write!(f, "uncaught Java exception ({reference:?})"),
            Self::Exit(status) => write!(f, "process exited with status {status}"),
            Self::Host(message) => write!(f, "host error: {message}"),
            Self::InvalidCode { context, message } => {
                write!(f, "invalid bytecode in {context}: {message}")
            }
            Self::Internal(message) => write!(f, "internal VM error: {message}"),
        }
    }
}
