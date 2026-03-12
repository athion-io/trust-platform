
use std::collections::HashMap;


/// Request to evaluate an expression
#[derive(Debug, Deserialize)]
pub struct EvaluateExpressionRequest {
    /// The ST expression to evaluate
    pub expression: String,
    /// Variables available for substitution
    pub variables: HashMap<String, VariableJson>,
}

/// Variable representation for JSON serialization
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VariableJson {
    /// Variable value
    pub value: ValueJson,
}

/// Value types for JSON serialization
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum ValueJson {
    #[serde(rename = "bool")]
    Bool(bool),
    #[serde(rename = "int")]
    Int(i64),
    #[serde(rename = "real")]
    Real(f64),
    #[serde(rename = "string")]
    String(String),
    #[serde(rename = "struct")]
    Struct(HashMap<String, ValueJson>),
}

/// Response from expression evaluation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvaluateExpressionResponse {
    /// Whether the evaluation succeeded
    pub success: bool,
    /// The resulting value if successful
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<ValueJson>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl From<Value> for ValueJson {
    fn from(value: Value) -> Self {
        match value {
            Value::Bool(b) => ValueJson::Bool(b),
            Value::Int(i) => ValueJson::Int(i),
            Value::Real(r) => ValueJson::Real(r),
            Value::String(s) => ValueJson::String(s),
            Value::Struct(fields) => ValueJson::Struct(
                fields
                    .into_iter()
                    .map(|(name, value)| (name, ValueJson::from(value)))
                    .collect(),
            ),
        }
    }
}

impl From<ValueJson> for Value {
    fn from(value: ValueJson) -> Self {
        match value {
            ValueJson::Bool(b) => Value::Bool(b),
            ValueJson::Int(i) => Value::Int(i),
            ValueJson::Real(r) => Value::Real(r),
            ValueJson::String(s) => Value::String(s),
            ValueJson::Struct(fields) => Value::Struct(
                fields
                    .into_iter()
                    .map(|(name, value)| (name, Value::from(value)))
                    .collect(),
            ),
        }
    }
}

impl From<Variable> for VariableJson {
    fn from(var: Variable) -> Self {
        VariableJson {
            value: var.value().clone().into(),
        }
    }
}

impl From<VariableJson> for Variable {
    fn from(var: VariableJson) -> Self {
        Variable::from(Value::from(var.value))
    }
}

#[cfg_attr(all(target_arch = "wasm32", feature = "wasm"), wasm_bindgen)]
impl WasmAnalysisEngine {
    /// Evaluate an ST expression with given variables
    ///
    /// # Request Format
    /// ```json
    /// {
    ///   "expression": "temperature > 25.0 AND enabled",
    ///   "variables": {
    ///     "temperature": { "value": { "type": "real", "value": 30.5 } },
    ///     "enabled": { "value": { "type": "bool", "value": true } }
    ///   }
    /// }
    /// ```
    ///
    /// # Response Format
    /// ```json
    /// {
    ///   "success": true,
    ///   "value": { "type": "bool", "value": true }
    /// }
    /// ```
    #[cfg_attr(
        all(target_arch = "wasm32", feature = "wasm"),
        wasm_bindgen(js_name = evaluateExpressionJson)
    )]
    pub fn evaluate_expression_json(&self, request_json: &str) -> Result<String, String> {
        // Deserialize request
        let request: EvaluateExpressionRequest = serde_json::from_str(request_json)
            .map_err(|err| format!("Invalid request JSON: {}", err))?;

        // Convert variables from JSON format to internal format
        let variables: HashMap<String, Variable> = request
            .variables
            .into_iter()
            .map(|(name, var_json)| (name, Variable::from(var_json)))
            .collect();

        // Create evaluator and evaluate expression
        let evaluator = Evaluator::new();
        let response = match evaluator.eval(&request.expression, &variables) {
            Ok(result) => EvaluateExpressionResponse {
                success: true,
                value: Some(result.value().clone().into()),
                error: None,
            },
            Err(err) => EvaluateExpressionResponse {
                success: false,
                value: None,
                error: Some(err.to_string()),
            },
        };

        // Serialize response
        json_string(&response)
    }
}