// File: crates/trust-expr-eval/src/types.rs

use serde::{Deserialize, Serialize};
use std::fmt;

/// Runtime value that can be stored in a variable
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    Bool(bool),
    Int(i64),
    Real(f64),
    String(String),
}

impl Value {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_real(&self) -> Option<f64> {
        match self {
            Value::Real(r) => Some(*r),
            Value::Int(i) => Some(*i as f64), // Implicit conversion
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Bool(_) => "BOOL",
            Value::Int(_) => "INT",
            Value::Real(_) => "REAL",
            Value::String(_) => "STRING",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{}", if *b { "TRUE" } else { "FALSE" }),
            Value::Int(i) => write!(f, "{}", i),
            Value::Real(r) => write!(f, "{}", r),
            Value::String(s) => write!(f, "'{}'", s),
        }
    }
}

/// Typed variable with value
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variable {
    value: Value,
}

impl Variable {
    pub fn bool(b: bool) -> Self {
        Self { value: Value::Bool(b) }
    }

    pub fn int(i: i64) -> Self {
        Self { value: Value::Int(i) }
    }

    pub fn real(r: f64) -> Self {
        Self { value: Value::Real(r) }
    }

    pub fn string(s: impl Into<String>) -> Self {
        Self { value: Value::String(s.into()) }
    }

    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn into_value(self) -> Value {
        self.value
    }
}

impl From<bool> for Variable {
    fn from(b: bool) -> Self {
        Self::bool(b)
    }
}

impl From<i64> for Variable {
    fn from(i: i64) -> Self {
        Self::int(i)
    }
}

impl From<f64> for Variable {
    fn from(r: f64) -> Self {
        Self::real(r)
    }
}

impl From<String> for Variable {
    fn from(s: String) -> Self {
        Self::string(s)
    }
}

impl From<&str> for Variable {
    fn from(s: &str) -> Self {
        Self::string(s)
    }
}

impl From<Value> for Variable {
    fn from(value: Value) -> Self {
        Self { value }
    }
}