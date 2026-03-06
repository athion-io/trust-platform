use crate::error::{EvalError, Result};
use crate::types::{Value, Variable};
use trust_syntax::parser::parse;
use trust_syntax::syntax::{SyntaxKind, SyntaxNode};
use std::collections::HashMap;

/// Expression evaluator
pub struct Evaluator {}

impl Evaluator {
    pub fn new() -> Self {
        Self {}
    }

    pub fn eval(
        &self,
        expression: &str,
        variables: &HashMap<String, Variable>,
    ) -> Result<Variable> {
        // Wrap expression in minimal program context for parser
        let wrapped = format!(
            "PROGRAM __Eval__ VAR __dummy__: INT; END_VAR __dummy__ := {}; END_PROGRAM",
            expression
        );

        let parse = parse(&wrapped);
                    
        if !parse.errors().is_empty() {
            let err = &parse.errors()[0];
            return Err(EvalError::SyntaxError(format!("{:?}", err)));
        }

        let root = parse.syntax();

                // Debug: print the AST structure
        #[cfg(debug_assertions)]
        self.debug_print_tree(&root, 0);
        
        // Find the expression in the assignment statement
        let expr_node = self.find_expression(&root)?;
        let value = self.eval_node(&expr_node, variables)?;
        Ok(Variable::from(value))
    }

        /// Debug helper to visualize AST structure
    #[cfg(debug_assertions)]
    fn debug_print_tree(&self, node: &SyntaxNode, depth: usize) {
        let indent = "  ".repeat(depth);
        println!("{}Node: {:?}", indent, node.kind());
        
        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    self.debug_print_tree(&n, depth + 1);
                }
                rowan::NodeOrToken::Token(t) => {
                    if !t.kind().is_trivia() {
                        println!("{}  Token: {:?} = '{}'", indent, t.kind(), t.text());
                    }
                }
            }
        }
    }

    /// Find the expression node in the wrapped program
    fn find_expression(&self, root: &SyntaxNode) -> Result<SyntaxNode> {
        // Find the AssignStmt node and extract the RHS expression
        for node in root.descendants() {
            if node.kind() == SyntaxKind::AssignStmt {
                // Get all child nodes (not tokens)
                let child_nodes: Vec<_> = node.children().collect();
                
                if child_nodes.len() >= 2 {
                    // Index 0: NameRef (LHS variable)
                    // Index 1: Expression (RHS - what we want)
                    return Ok(child_nodes[1].clone());
                } else if child_nodes.len() == 1 {
                    // Only one child means it's the expression itself
                    return Ok(child_nodes[0].clone());
                }
            }
        }
        
        Err(EvalError::SyntaxError("Could not find expression in parsed tree".to_string()))
    }

    /// Recursively evaluate AST node
    fn eval_node(
        &self,
        node: &SyntaxNode,
        variables: &HashMap<String, Variable>,
    ) -> Result<Value> {
        match node.kind() {
            // Literals
            SyntaxKind::Literal => {
                // Literal contains the actual value as a child token
                for child in node.children_with_tokens() {
                    if let Some(token) = child.as_token() {
                        let text = token.text().trim();
                        
                        // Try integer
                        if let Ok(val) = text.parse::<i64>() {
                            return Ok(Value::Int(val));
                        }
                        
                        // Try real
                        if let Ok(val) = text.parse::<f64>() {
                            return Ok(Value::Real(val));
                        }
                        
                        // Try boolean
                        match text.to_uppercase().as_str() {
                            "TRUE" => return Ok(Value::Bool(true)),
                            "FALSE" => return Ok(Value::Bool(false)),
                            _ => {}
                        }
                        
                        // String literal
                        if (text.starts_with('\'') && text.ends_with('\'')) 
                           || (text.starts_with('"') && text.ends_with('"')) {
                            let s = text[1..text.len()-1].to_string();
                            return Ok(Value::String(s));
                        }
                    }
                }
                Err(EvalError::SyntaxError("Invalid literal".to_string()))
            }

            // Variable reference
            SyntaxKind::NameRef | SyntaxKind::Name => {
                // Get the identifier token
                for child in node.children_with_tokens() {
                    if let Some(token) = child.as_token() {
                        if token.kind() == SyntaxKind::Ident {
                            let name = token.text();
                            return variables
                                .get(name)
                                .map(|var| var.value().clone())
                                .ok_or_else(|| EvalError::UndefinedVariable(name.to_string()));
                        }
                    }
                }
                Err(EvalError::SyntaxError("Invalid name reference".to_string()))
            }

            // Binary expressions
            SyntaxKind::BinaryExpr => {
                self.eval_binary(node, variables)
            }

            // Unary expressions
            SyntaxKind::UnaryExpr => {
                self.eval_unary(node, variables)
            }

            // Parenthesized expressions
            SyntaxKind::ParenExpr => {
                // Find the inner expression (skip parentheses tokens)
                for child in node.children() {
                    return self.eval_node(&child, variables);
                }
                Err(EvalError::SyntaxError("Empty parentheses".to_string()))
            }

            _ => {
                // Try to find evaluable child nodes
                for child in node.children() {
                    if let Ok(result) = self.eval_node(&child, variables) {
                        return Ok(result);
                    }
                }
                Err(EvalError::UnsupportedOperation(format!(
                    "Cannot evaluate node kind: {:?}",
                    node.kind()
                )))
            }
        }
    }

    /// Evaluate binary expression
    fn eval_binary(
        &self,
        node: &SyntaxNode,
        variables: &HashMap<String, Variable>,
    ) -> Result<Value> {
        let mut operands = Vec::new();
        let mut operator = None;

        // Collect operands and operator
        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    operands.push(self.eval_node(&n, variables)?);
                }
                rowan::NodeOrToken::Token(t) => {
                    // Skip trivia
                    if !t.kind().is_trivia() && operator.is_none() {
                        operator = Some(t.text().to_string());
                    }
                }
            }
        }

        if operands.len() != 2 {
            return Err(EvalError::SyntaxError(
                "Binary expression must have exactly 2 operands".to_string()
            ));
        }

        let op = operator.ok_or_else(|| {
            EvalError::SyntaxError("Binary expression missing operator".to_string())
        })?;

        self.apply_binary_op(&operands[0], &op, &operands[1])
    }

    /// Evaluate unary expression
    fn eval_unary(
        &self,
        node: &SyntaxNode,
        variables: &HashMap<String, Variable>,
    ) -> Result<Value> {
        let mut operand = None;
        let mut operator = None;

        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    operand = Some(self.eval_node(&n, variables)?);
                }
                rowan::NodeOrToken::Token(t) => {
                    if !t.kind().is_trivia() && operator.is_none() {
                        operator = Some(t.text().to_string());
                    }
                }
            }
        }

        let operand = operand.ok_or_else(|| {
            EvalError::SyntaxError("Unary expression missing operand".to_string())
        })?;

        let op = operator.ok_or_else(|| {
            EvalError::SyntaxError("Unary expression missing operator".to_string())
        })?;

        self.apply_unary_op(&op, &operand)
    }

    /// Apply binary operator to two values
    fn apply_binary_op(&self, left: &Value, op: &str, right: &Value) -> Result<Value> {
        match op.to_uppercase().as_str() {
            // Arithmetic
            "+" => self.add(left, right),
            "-" => self.subtract(left, right),
            "*" => self.multiply(left, right),
            "/" => self.divide(left, right),

            // Comparison
            "=" => Ok(Value::Bool(left == right)),
            "<>" => Ok(Value::Bool(left != right)),
            "<" => self.less_than(left, right),
            ">" => self.greater_than(left, right),
            "<=" => self.less_equal(left, right),
            ">=" => self.greater_equal(left, right),

            // Logical
            "AND" => self.logical_and(left, right),
            "OR" => self.logical_or(left, right),
            "XOR" => self.logical_xor(left, right),

            _ => Err(EvalError::UnsupportedOperation(format!(
                "Unknown operator: {}",
                op
            ))),
        }
    }

    /// Apply unary operator to a value
    fn apply_unary_op(&self, op: &str, operand: &Value) -> Result<Value> {
        match op.to_uppercase().as_str() {
            "NOT" => match operand {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err(EvalError::type_error("BOOL", operand.type_name())),
            },

            "-" => match operand {
                Value::Int(i) => Ok(Value::Int(-i)),
                Value::Real(r) => Ok(Value::Real(-r)),
                _ => Err(EvalError::type_error("number", operand.type_name())),
            },

            _ => Err(EvalError::UnsupportedOperation(format!(
                "Unknown unary operator: {}",
                op
            ))),
        }
    }

    // Arithmetic operations
    fn add(&self, left: &Value, right: &Value) -> Result<Value> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (a, b) => {
                let a = a.as_real().ok_or_else(|| EvalError::type_error("number", a.type_name()))?;
                let b = b.as_real().ok_or_else(|| EvalError::type_error("number", b.type_name()))?;
                Ok(Value::Real(a + b))
            }
        }
    }

    fn subtract(&self, left: &Value, right: &Value) -> Result<Value> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (a, b) => {
                let a = a.as_real().ok_or_else(|| EvalError::type_error("number", a.type_name()))?;
                let b = b.as_real().ok_or_else(|| EvalError::type_error("number", b.type_name()))?;
                Ok(Value::Real(a - b))
            }
        }
    }

    fn multiply(&self, left: &Value, right: &Value) -> Result<Value> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (a, b) => {
                let a = a.as_real().ok_or_else(|| EvalError::type_error("number", a.type_name()))?;
                let b = b.as_real().ok_or_else(|| EvalError::type_error("number", b.type_name()))?;
                Ok(Value::Real(a * b))
            }
        }
    }

    fn divide(&self, left: &Value, right: &Value) -> Result<Value> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                if *b == 0 {
                    return Err(EvalError::DivisionByZero);
                }
                Ok(Value::Int(a / b))
            }
            (a, b) => {
                let a = a.as_real().ok_or_else(|| EvalError::type_error("number", a.type_name()))?;
                let b = b.as_real().ok_or_else(|| EvalError::type_error("number", b.type_name()))?;
                if b == 0.0 {
                    return Err(EvalError::DivisionByZero);
                }
                Ok(Value::Real(a / b))
            }
        }
    }

    // Comparison operations
    fn less_than(&self, left: &Value, right: &Value) -> Result<Value> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
            (a, b) => {
                let a = a.as_real().ok_or_else(|| EvalError::type_error("number", a.type_name()))?;
                let b = b.as_real().ok_or_else(|| EvalError::type_error("number", b.type_name()))?;
                Ok(Value::Bool(a < b))
            }
        }
    }

    fn greater_than(&self, left: &Value, right: &Value) -> Result<Value> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
            (a, b) => {
                let a = a.as_real().ok_or_else(|| EvalError::type_error("number", a.type_name()))?;
                let b = b.as_real().ok_or_else(|| EvalError::type_error("number", b.type_name()))?;
                Ok(Value::Bool(a > b))
            }
        }
    }

    fn less_equal(&self, left: &Value, right: &Value) -> Result<Value> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
            (a, b) => {
                let a = a.as_real().ok_or_else(|| EvalError::type_error("number", a.type_name()))?;
                let b = b.as_real().ok_or_else(|| EvalError::type_error("number", b.type_name()))?;
                Ok(Value::Bool(a <= b))
            }
        }
    }

    fn greater_equal(&self, left: &Value, right: &Value) -> Result<Value> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
            (a, b) => {
                let a = a.as_real().ok_or_else(|| EvalError::type_error("number", a.type_name()))?;
                let b = b.as_real().ok_or_else(|| EvalError::type_error("number", b.type_name()))?;
                Ok(Value::Bool(a >= b))
            }
        }
    }

    // Logical operations
    fn logical_and(&self, left: &Value, right: &Value) -> Result<Value> {
        match (left, right) {
            (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a && *b)),
            (a, b) => Err(EvalError::type_error(
                "BOOL",
                &format!("{} AND {}", a.type_name(), b.type_name()),
            )),
        }
    }

    fn logical_or(&self, left: &Value, right: &Value) -> Result<Value> {
        match (left, right) {
            (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a || *b)),
            (a, b) => Err(EvalError::type_error(
                "BOOL",
                &format!("{} OR {}", a.type_name(), b.type_name()),
            )),
        }
    }

    fn logical_xor(&self, left: &Value, right: &Value) -> Result<Value> {
        match (left, right) {
            (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a ^ *b)),
            (a, b) => Err(EvalError::type_error(
                "BOOL",
                &format!("{} XOR {}", a.type_name(), b.type_name()),
            )),
        }
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}