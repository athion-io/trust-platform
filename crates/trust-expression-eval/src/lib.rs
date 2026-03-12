// File: crates/trust-expr-eval/src/lib.rs

//! Expression evaluation engine for Structured Text.
//! 
//! This crate provides runtime evaluation of ST expressions with variable substitution.
//! It does NOT execute full programs—only individual expressions.
//!
//! # Example
//! ```
//! use trust_expression_eval::{Evaluator, Variable};
//! use std::collections::HashMap;
//!
//! let mut vars = HashMap::new();
//! vars.insert("temperature".to_string(), Variable::real(25.5));
//! vars.insert("threshold".to_string(), Variable::real(20.0));
//!
//! let evaluator = Evaluator::new();
//! let result = evaluator.eval("temperature > threshold", &vars).unwrap();
//! 
//! assert_eq!(result, Variable::bool(true));
//! ```

mod evaluator;
mod types;
mod error;

// Re-export public API
pub use evaluator::Evaluator;
pub use types::{Variable, Value};
pub use error::{EvalError, Result};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_basic_arithmetic() {
        let evaluator = Evaluator::new();
        let vars = HashMap::new();
        
        assert_eq!(
            evaluator.eval("5 + 3", &vars).unwrap(),
            Variable::int(8)
        );
        
        assert_eq!(
            evaluator.eval("10.0 * 2.5", &vars).unwrap(),
            Variable::real(25.0)
        );
    }

    #[test]
    fn test_variable_substitution() {
        let evaluator = Evaluator::new();
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), Variable::int(10));
        vars.insert("y".to_string(), Variable::int(20));
        
        assert_eq!(
            evaluator.eval("x + y", &vars).unwrap(),
            Variable::int(30)
        );
    }

    #[test]
    fn test_comparison() {
        let evaluator = Evaluator::new();
        let mut vars = HashMap::new();
        vars.insert("temp".to_string(), Variable::real(25.5));
        
        assert_eq!(
            evaluator.eval("temp > 20.0", &vars).unwrap(),
            Variable::bool(true)
        );
        
        assert_eq!(
            evaluator.eval("temp < 25.0", &vars).unwrap(),
            Variable::bool(false)
        );
    }

    #[test]
    fn test_logical_operators() {
        let evaluator = Evaluator::new();
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), Variable::bool(true));
        vars.insert("b".to_string(), Variable::bool(false));
        
        assert_eq!(
            evaluator.eval("a AND b", &vars).unwrap(),
            Variable::bool(false)
        );
        
        assert_eq!(
            evaluator.eval("a OR b", &vars).unwrap(),
            Variable::bool(true)
        );
        
        assert_eq!(
            evaluator.eval("NOT a", &vars).unwrap(),
            Variable::bool(false)
        );
    }

    #[test]
    fn test_complex_expression() {
        let evaluator = Evaluator::new();
        let mut vars = HashMap::new();
        vars.insert("temperature".to_string(), Variable::real(30.5));
        vars.insert("pressure".to_string(), Variable::real(85.3));
        vars.insert("enabled".to_string(), Variable::bool(true));
        
        let expr = "enabled AND (temperature > 25.0 OR pressure < 100.0)";
        assert_eq!(
            evaluator.eval(expr, &vars).unwrap(),
            Variable::bool(true)
        );
    }

    #[test]
    fn test_undefined_variable() {
        let evaluator = Evaluator::new();
        let vars = HashMap::new();
        
        let result = evaluator.eval("unknown_var + 5", &vars);
        assert!(result.is_err());
    }

    #[test]
    fn test_type_mismatch() {
        let evaluator = Evaluator::new();
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), Variable::bool(true));
        
        let result = evaluator.eval("x + 5", &vars);
        assert!(result.is_err());
    }

    #[test]
    fn test_struct_field_access() {
        let evaluator = Evaluator::new();
        let mut vars = HashMap::new();
        vars.insert(
            "device".to_string(),
            Variable::structure(HashMap::from([
                ("enabled".to_string(), Value::Bool(true)),
                ("count".to_string(), Value::Int(7)),
                ("name".to_string(), Value::String("pump-a".to_string())),
            ])),
        );

        assert_eq!(
            evaluator.eval("device.count + 3", &vars).unwrap(),
            Variable::int(10)
        );

        assert_eq!(
            evaluator.eval("device.enabled AND TRUE", &vars).unwrap(),
            Variable::bool(true)
        );

        assert_eq!(
            evaluator.eval("device.name = 'pump-a'", &vars).unwrap(),
            Variable::bool(true)
        );
    }

    #[test]
    fn test_nested_struct_field_access() {
        let evaluator = Evaluator::new();
        let mut vars = HashMap::new();
        vars.insert(
            "outer".to_string(),
            Variable::structure(HashMap::from([(
                "inner".to_string(),
                Value::Struct(HashMap::from([
                    ("ok".to_string(), Value::Bool(true)),
                    ("value".to_string(), Value::Int(12)),
                ])),
            )])),
        );

        assert_eq!(
            evaluator.eval("outer.inner.ok", &vars).unwrap(),
            Variable::bool(true)
        );

        assert_eq!(
            evaluator.eval("outer.inner.value > 10", &vars).unwrap(),
            Variable::bool(true)
        );
    }

    #[test]
    fn test_field_access_on_non_struct_errors() {
        let evaluator = Evaluator::new();
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), Variable::int(1));

        let result = evaluator.eval("x.field", &vars);
        assert!(result.is_err());
    }
}