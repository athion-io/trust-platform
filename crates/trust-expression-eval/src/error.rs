// File: crates/trust-expr-eval/src/error.rs

use thiserror::Error;

pub type Result<T> = std::result::Result<T, EvalError>;

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("Syntax error: {0}")]
    SyntaxError(String),

    #[error("Variable '{0}' not found")]
    UndefinedVariable(String),

    #[error("Type error: expected {expected}, got {actual}")]
    TypeError {
        expected: String,
        actual: String,
    },

    #[error("Division by zero")]
    DivisionByZero,

    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("Evaluation error: {0}")]
    RuntimeError(String),
}

impl EvalError {
    pub fn type_error(expected: &str, actual: &str) -> Self {
        Self::TypeError {
            expected: expected.to_string(),
            actual: actual.to_string(),
        }
    }
}