#[cfg(test)]
mod tests {
    use super::{offset_to_position, position_to_offset, Position};
    use super::{EvaluateExpressionResponse, ValueJson};

    #[test]
    fn line_character_offset_roundtrip_ascii() {
        let source = "PROGRAM Main\nVAR\n  x : INT;\nEND_VAR\n";
        let position = Position {
            line: 2,
            character: 2,
        };
        let offset = position_to_offset(source, position.clone()).expect("offset");
        let roundtrip = offset_to_position(source, offset);
        assert_eq!(roundtrip, position);
    }

    #[test]
    fn line_character_offset_roundtrip_utf16() {
        let source = "PROGRAM Main\nVAR\n  emoji : STRING := '😀';\nEND_VAR\n";
        let position = Position {
            line: 2,
            character: 25,
        };
        let offset = position_to_offset(source, position.clone()).expect("offset");
        let roundtrip = offset_to_position(source, offset);
        assert_eq!(roundtrip, position);
    }

    #[test]
    fn position_to_offset_clamps_inside_utf16_surrogate_pair() {
        let source = "😀a";
        let offset = position_to_offset(
            source,
            Position {
                line: 0,
                character: 1,
            },
        )
        .expect("offset");
        assert_eq!(offset, 0);
    }

        #[test]
    fn test_evaluate_simple_arithmetic() {
        let engine = crate::WasmAnalysisEngine::default();
        
        let request = r#"{
            "expression": "5 + 3",
            "variables": {}
        }"#;

        let response = engine.evaluate_expression_json(request).unwrap();
        let resp: EvaluateExpressionResponse = serde_json::from_str(&response).unwrap();
        
        assert!(resp.success);
        assert!(matches!(resp.value, Some(ValueJson::Int(8))));
    }

    #[test]
    fn test_evaluate_with_variables() {
        let engine = crate::WasmAnalysisEngine::default();
        
        let request = r#"{
            "expression": "x + y",
            "variables": {
                "x": { "value": { "type": "int", "value": 10 } },
                "y": { "value": { "type": "int", "value": 20 } }
            }
        }"#;

        let response = engine.evaluate_expression_json(request).unwrap();
        let resp: EvaluateExpressionResponse = serde_json::from_str(&response).unwrap();
        
        assert!(resp.success);
        assert!(matches!(resp.value, Some(ValueJson::Int(30))));
    }

    #[test]
    fn test_evaluate_comparison() {
        let engine = crate::WasmAnalysisEngine::default();
        
        let request = r#"{
            "expression": "temperature > 25.0",
            "variables": {
                "temperature": { "value": { "type": "real", "value": 30.5 } }
            }
        }"#;

        let response = engine.evaluate_expression_json(request).unwrap();
        let resp: EvaluateExpressionResponse = serde_json::from_str(&response).unwrap();
        
        assert!(resp.success);
        assert!(matches!(resp.value, Some(ValueJson::Bool(true))));
    }

    #[test]
    fn test_evaluate_logical() {
        let engine = crate::WasmAnalysisEngine::default();
        
        let request = r#"{
            "expression": "enabled AND NOT fault",
            "variables": {
                "enabled": { "value": { "type": "bool", "value": true } },
                "fault": { "value": { "type": "bool", "value": false } }
            }
        }"#;

        let response = engine.evaluate_expression_json(request).unwrap();
        let resp: EvaluateExpressionResponse = serde_json::from_str(&response).unwrap();
        
        assert!(resp.success);
        assert!(matches!(resp.value, Some(ValueJson::Bool(true))));
    }

    #[test]
    fn test_evaluate_undefined_variable() {
        let engine = crate::WasmAnalysisEngine::default();
        
        let request = r#"{
            "expression": "unknown_var + 5",
            "variables": {}
        }"#;

        let response = engine.evaluate_expression_json(request).unwrap();
        let resp: EvaluateExpressionResponse = serde_json::from_str(&response).unwrap();
        
        assert!(!resp.success);
        assert!(resp.error.is_some());
        
        // Debug: print the actual error message
        let error_msg = resp.error.as_ref().unwrap();
        println!("Actual error message: {}", error_msg);
        
        // Check for the actual error message format
        let error_lower = error_msg.to_lowercase();
        assert!(
            error_lower.contains("undefined") || error_lower.contains("unknown_var"),
            "Expected error about undefined variable, got: {}",
            error_msg
        );
    }
}
