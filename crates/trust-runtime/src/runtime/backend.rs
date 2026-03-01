//! Runtime execution backend dispatch seam.

use crate::error;
use crate::execution_backend::ExecutionBackend;
use crate::task::ProgramDef;
use crate::value::ValueRef;

use super::core::Runtime;

/// Backend contract for runtime execution paths.
pub(super) trait RuntimeExecutionBackend {
    fn execute_program(
        &self,
        runtime: &mut Runtime,
        program: &ProgramDef,
    ) -> Result<(), error::RuntimeError>;

    fn execute_function_block_ref(
        &self,
        runtime: &mut Runtime,
        reference: &ValueRef,
    ) -> Result<(), error::RuntimeError>;
}

struct InterpreterBackend;
struct BytecodeVmBackend;

static INTERPRETER_BACKEND: InterpreterBackend = InterpreterBackend;
static BYTECODE_VM_BACKEND: BytecodeVmBackend = BytecodeVmBackend;

pub(super) fn resolve_backend(mode: ExecutionBackend) -> &'static dyn RuntimeExecutionBackend {
    match mode {
        ExecutionBackend::Interpreter => &INTERPRETER_BACKEND,
        ExecutionBackend::BytecodeVm => &BYTECODE_VM_BACKEND,
    }
}

pub(super) fn validate_backend_selection(
    runtime: &Runtime,
    mode: ExecutionBackend,
) -> Result<(), error::RuntimeError> {
    if matches!(mode, ExecutionBackend::BytecodeVm) && runtime.vm_module.is_none() {
        return Err(error::RuntimeError::InvalidConfig(
            "runtime.execution_backend='vm' requires loaded bytecode module".into(),
        ));
    }
    Ok(())
}

impl RuntimeExecutionBackend for InterpreterBackend {
    fn execute_program(
        &self,
        runtime: &mut Runtime,
        program: &ProgramDef,
    ) -> Result<(), error::RuntimeError> {
        runtime.execute_program_interpreter(program)
    }

    fn execute_function_block_ref(
        &self,
        runtime: &mut Runtime,
        reference: &ValueRef,
    ) -> Result<(), error::RuntimeError> {
        runtime.execute_function_block_ref_interpreter(reference)
    }
}

impl RuntimeExecutionBackend for BytecodeVmBackend {
    fn execute_program(
        &self,
        runtime: &mut Runtime,
        program: &ProgramDef,
    ) -> Result<(), error::RuntimeError> {
        super::vm::execute_program(runtime, program)
    }

    fn execute_function_block_ref(
        &self,
        runtime: &mut Runtime,
        reference: &ValueRef,
    ) -> Result<(), error::RuntimeError> {
        super::vm::execute_function_block_ref(runtime, reference)
    }
}
