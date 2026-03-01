use trust_runtime::execution_backend::ExecutionBackend;
use trust_runtime::Runtime;

#[test]
fn loads_runtime() {
    let runtime = Runtime::new();
    let _profile = runtime.profile();
}

#[test]
fn runtime_execution_backend_defaults_and_validation() {
    let mut runtime = Runtime::new();
    assert_eq!(runtime.execution_backend(), ExecutionBackend::Interpreter);

    let err = runtime
        .set_execution_backend(ExecutionBackend::BytecodeVm)
        .expect_err("vm backend should require loaded bytecode module");
    assert!(err.to_string().contains("runtime.execution_backend='vm'"));
}
