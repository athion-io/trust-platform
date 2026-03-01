use std::time::Instant;

use smol_str::SmolStr;

use crate::debug::DebugHook;
use crate::error::RuntimeError;
use crate::eval::ops::{BinaryOp, UnaryOp};
use crate::memory::InstanceId;
use crate::task::ProgramDef;
use crate::value::{size_of_value, Value, ValueRef};

use super::super::core::Runtime;
use super::call::{execute_native_call, push_call_frame};
use super::dispatch_ops::{apply_jump, execute_binary, execute_unary, read_i32, read_u32};
use super::dispatch_refs::{
    dynamic_load_ref, dynamic_ref_field, dynamic_ref_index, dynamic_store_ref, index_to_i64,
    load_ref, load_ref_addr, pop_reference, store_ref,
};
use super::dispatch_sizeof::{sizeof_error_to_runtime, sizeof_type_from_table};
use super::errors::VmTrap;
use super::frames::FrameStack;
use super::stack::OperandStack;
use super::VmModule;

pub(super) fn execute_program(
    runtime: &mut Runtime,
    program: &ProgramDef,
) -> Result<(), RuntimeError> {
    let module = runtime.vm_module.clone().ok_or_else(|| {
        RuntimeError::InvalidConfig(
            "runtime.execution_backend='vm' requires loaded bytecode module".into(),
        )
    })?;

    let key = SmolStr::new(program.name.to_ascii_uppercase());
    let pou_id = module
        .program_ids
        .get(&key)
        .copied()
        .ok_or_else(|| VmTrap::MissingProgram(program.name.clone()).into_runtime_error())?;

    let instance_id = match runtime.storage.get_global(program.name.as_ref()) {
        Some(Value::Instance(id)) => Some(*id),
        _ => None,
    };

    execute_pou(runtime, module.as_ref(), pou_id, instance_id)
}

pub(super) fn execute_function_block_ref(
    runtime: &mut Runtime,
    reference: &ValueRef,
) -> Result<(), RuntimeError> {
    let module = runtime.vm_module.clone().ok_or_else(|| {
        RuntimeError::InvalidConfig(
            "runtime.execution_backend='vm' requires loaded bytecode module".into(),
        )
    })?;

    let instance_id = match runtime.storage.read_by_ref(reference.clone()) {
        Some(Value::Instance(id)) => *id,
        Some(_) => return Err(RuntimeError::TypeMismatch),
        None => return Err(RuntimeError::NullReference),
    };

    let instance = runtime
        .storage
        .get_instance(instance_id)
        .ok_or(RuntimeError::NullReference)?;
    let key = SmolStr::new(instance.type_name.to_ascii_uppercase());
    let pou_id = module
        .function_block_ids
        .get(&key)
        .copied()
        .ok_or_else(|| {
            VmTrap::MissingFunctionBlock(instance.type_name.clone()).into_runtime_error()
        })?;

    execute_pou(runtime, module.as_ref(), pou_id, Some(instance_id))
}

fn execute_pou(
    runtime: &mut Runtime,
    module: &VmModule,
    pou_id: u32,
    entry_instance: Option<InstanceId>,
) -> Result<(), RuntimeError> {
    let mut operand_stack = OperandStack::default();
    let mut frames = FrameStack::default();
    let mut pc = push_call_frame(&mut frames, module, pou_id, usize::MAX, entry_instance)
        .map_err(VmTrap::into_runtime_error)?;
    let mut budget = module.instruction_budget;

    loop {
        if frames.is_empty() {
            break;
        }

        let (frame_pou_id, frame_start, frame_end) = {
            let frame = frames
                .current()
                .ok_or_else(|| VmTrap::CallStackUnderflow.into_runtime_error())?;
            (frame.pou_id, frame.code_start, frame.code_end)
        };

        if pc == frame_end {
            let finished = frames.pop().map_err(VmTrap::into_runtime_error)?;
            if frames.is_empty() {
                break;
            }
            pc = finished.return_pc;
            continue;
        }

        if pc < frame_start || pc > frame_end {
            return Err(VmTrap::InvalidJumpTarget(pc as i64).into_runtime_error());
        }

        if budget == 0 {
            return Err(VmTrap::BudgetExceeded.into_runtime_error());
        }
        budget = budget.saturating_sub(1);

        if deadline_exceeded(runtime.execution_deadline) {
            return Err(VmTrap::DeadlineExceeded.into_runtime_error());
        }

        if let Some(location) = vm_statement_location(runtime, module, frame_pou_id, pc) {
            if let Some(debug) = runtime.debug.as_mut() {
                let call_depth = frames.len().saturating_sub(1) as u32;
                debug.on_statement(Some(&location), call_depth);
            }
        }

        let opcode = module
            .code
            .get(pc)
            .copied()
            .ok_or_else(|| VmTrap::BytecodeDecode("vm instruction fetch out of bounds".into()))
            .map_err(VmTrap::into_runtime_error)?;
        pc += 1;

        match opcode {
            0x00 => {}
            0x01 => return Err(VmTrap::ForStepZero.into_runtime_error()),
            0x02 => {
                let offset = read_i32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let frame = frames
                    .current()
                    .ok_or_else(|| VmTrap::CallStackUnderflow.into_runtime_error())?;
                apply_jump(&mut pc, offset, frame).map_err(VmTrap::into_runtime_error)?;
            }
            0x03 | 0x04 => {
                let offset = read_i32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let condition = operand_stack.pop().map_err(VmTrap::into_runtime_error)?;
                let condition = match condition {
                    Value::Bool(value) => value,
                    _ => return Err(VmTrap::ConditionNotBool.into_runtime_error()),
                };
                let should_jump = (opcode == 0x03 && condition) || (opcode == 0x04 && !condition);
                if should_jump {
                    let frame = frames
                        .current()
                        .ok_or_else(|| VmTrap::CallStackUnderflow.into_runtime_error())?;
                    apply_jump(&mut pc, offset, frame).map_err(VmTrap::into_runtime_error)?;
                }
            }
            0x05 => {
                let callee = read_u32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let inherited_instance = frames.current().and_then(|frame| frame.runtime_instance);
                let return_pc = pc;
                pc = push_call_frame(&mut frames, module, callee, return_pc, inherited_instance)
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x06 => {
                let finished = frames.pop().map_err(VmTrap::into_runtime_error)?;
                if frames.is_empty() {
                    break;
                }
                pc = finished.return_pc;
            }
            0x07 => return Err(VmTrap::UnsupportedOpcode("CALL_METHOD").into_runtime_error()),
            0x08 => return Err(VmTrap::UnsupportedOpcode("CALL_VIRTUAL").into_runtime_error()),
            0x09 => {
                let kind = read_u32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let symbol_idx =
                    read_u32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let arg_count =
                    read_u32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let frame = frames
                    .current_mut()
                    .ok_or_else(|| VmTrap::CallStackUnderflow.into_runtime_error())?;
                let result = execute_native_call(
                    runtime,
                    module,
                    frame,
                    &mut operand_stack,
                    kind,
                    symbol_idx,
                    arg_count,
                )
                .map_err(VmTrap::into_runtime_error)?;
                operand_stack
                    .push(result)
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x10 => {
                let const_idx =
                    read_u32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let value = module
                    .consts
                    .get(const_idx as usize)
                    .cloned()
                    .ok_or(VmTrap::InvalidConstIndex(const_idx))
                    .map_err(VmTrap::into_runtime_error)?;
                operand_stack
                    .push(value)
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x11 => operand_stack
                .duplicate_top()
                .map_err(VmTrap::into_runtime_error)?,
            0x12 => {
                let _ = operand_stack.pop().map_err(VmTrap::into_runtime_error)?;
            }
            0x13 => operand_stack
                .swap_top()
                .map_err(VmTrap::into_runtime_error)?,
            0x14 => return Err(VmTrap::UnsupportedOpcode("ROT3").into_runtime_error()),
            0x15 => return Err(VmTrap::UnsupportedOpcode("ROT4").into_runtime_error()),
            0x16 => return Err(VmTrap::UnsupportedOpcode("CAST_IMPLICIT").into_runtime_error()),
            0x20 => {
                let ref_idx =
                    read_u32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let value = load_ref(runtime, module, &frames, ref_idx)
                    .map_err(VmTrap::into_runtime_error)?;
                operand_stack
                    .push(value)
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x21 => {
                let ref_idx =
                    read_u32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let value = operand_stack.pop().map_err(VmTrap::into_runtime_error)?;
                store_ref(runtime, module, &mut frames, ref_idx, value)
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x22 => {
                let ref_idx =
                    read_u32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let value_ref =
                    load_ref_addr(module, &frames, ref_idx).map_err(VmTrap::into_runtime_error)?;
                operand_stack
                    .push(Value::Reference(Some(value_ref)))
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x23 => {
                let frame = frames
                    .current()
                    .ok_or_else(|| VmTrap::CallStackUnderflow.into_runtime_error())?;
                let self_instance = frame.runtime_instance.ok_or_else(|| {
                    VmTrap::Runtime(RuntimeError::TypeMismatch).into_runtime_error()
                })?;
                operand_stack
                    .push(Value::Instance(self_instance))
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x24 => {
                let frame = frames
                    .current()
                    .ok_or_else(|| VmTrap::CallStackUnderflow.into_runtime_error())?;
                let self_instance = frame.runtime_instance.ok_or_else(|| {
                    VmTrap::Runtime(RuntimeError::TypeMismatch).into_runtime_error()
                })?;
                let instance = runtime
                    .storage
                    .get_instance(self_instance)
                    .ok_or_else(|| VmTrap::NullReference.into_runtime_error())?;
                let super_instance = instance.parent.ok_or_else(|| {
                    VmTrap::Runtime(RuntimeError::TypeMismatch).into_runtime_error()
                })?;
                operand_stack
                    .push(Value::Instance(super_instance))
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x30 => {
                let field_idx =
                    read_u32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let field = module
                    .strings
                    .get(field_idx as usize)
                    .cloned()
                    .ok_or_else(|| {
                        VmTrap::BytecodeDecode(
                            format!("invalid index {field_idx} for string").into(),
                        )
                        .into_runtime_error()
                    })?;
                let reference =
                    pop_reference(&mut operand_stack).map_err(VmTrap::into_runtime_error)?;
                let next = dynamic_ref_field(runtime, &frames, reference, field)
                    .map_err(VmTrap::into_runtime_error)?;
                operand_stack
                    .push(Value::Reference(Some(next)))
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x31 => {
                let index = operand_stack.pop().map_err(VmTrap::into_runtime_error)?;
                let index = index_to_i64(index).map_err(VmTrap::into_runtime_error)?;
                let reference =
                    pop_reference(&mut operand_stack).map_err(VmTrap::into_runtime_error)?;
                let next = dynamic_ref_index(runtime, &frames, reference, index)
                    .map_err(VmTrap::into_runtime_error)?;
                operand_stack
                    .push(Value::Reference(Some(next)))
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x32 => {
                let reference =
                    pop_reference(&mut operand_stack).map_err(VmTrap::into_runtime_error)?;
                let value = dynamic_load_ref(runtime, &frames, &reference)
                    .map_err(VmTrap::into_runtime_error)?;
                operand_stack
                    .push(value)
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x33 => {
                let value = operand_stack.pop().map_err(VmTrap::into_runtime_error)?;
                let reference =
                    pop_reference(&mut operand_stack).map_err(VmTrap::into_runtime_error)?;
                dynamic_store_ref(runtime, &mut frames, &reference, value)
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x40 => execute_binary(runtime, &mut operand_stack, BinaryOp::Add)
                .map_err(VmTrap::into_runtime_error)?,
            0x41 => execute_binary(runtime, &mut operand_stack, BinaryOp::Sub)
                .map_err(VmTrap::into_runtime_error)?,
            0x42 => execute_binary(runtime, &mut operand_stack, BinaryOp::Mul)
                .map_err(VmTrap::into_runtime_error)?,
            0x43 => execute_binary(runtime, &mut operand_stack, BinaryOp::Div)
                .map_err(VmTrap::into_runtime_error)?,
            0x44 => execute_binary(runtime, &mut operand_stack, BinaryOp::Mod)
                .map_err(VmTrap::into_runtime_error)?,
            0x45 => execute_unary(&mut operand_stack, UnaryOp::Neg)
                .map_err(VmTrap::into_runtime_error)?,
            0x46 => execute_binary(runtime, &mut operand_stack, BinaryOp::And)
                .map_err(VmTrap::into_runtime_error)?,
            0x47 => execute_binary(runtime, &mut operand_stack, BinaryOp::Or)
                .map_err(VmTrap::into_runtime_error)?,
            0x48 => execute_binary(runtime, &mut operand_stack, BinaryOp::Xor)
                .map_err(VmTrap::into_runtime_error)?,
            0x49 => execute_unary(&mut operand_stack, UnaryOp::Not)
                .map_err(VmTrap::into_runtime_error)?,
            0x4A => return Err(VmTrap::UnsupportedOpcode("SHL").into_runtime_error()),
            0x4B => return Err(VmTrap::UnsupportedOpcode("SHR").into_runtime_error()),
            0x4C => execute_binary(runtime, &mut operand_stack, BinaryOp::Pow)
                .map_err(VmTrap::into_runtime_error)?,
            0x4D => return Err(VmTrap::UnsupportedOpcode("ROL").into_runtime_error()),
            0x4E => return Err(VmTrap::UnsupportedOpcode("ROR").into_runtime_error()),
            0x50 => execute_binary(runtime, &mut operand_stack, BinaryOp::Eq)
                .map_err(VmTrap::into_runtime_error)?,
            0x51 => execute_binary(runtime, &mut operand_stack, BinaryOp::Ne)
                .map_err(VmTrap::into_runtime_error)?,
            0x52 => execute_binary(runtime, &mut operand_stack, BinaryOp::Lt)
                .map_err(VmTrap::into_runtime_error)?,
            0x53 => execute_binary(runtime, &mut operand_stack, BinaryOp::Le)
                .map_err(VmTrap::into_runtime_error)?,
            0x54 => execute_binary(runtime, &mut operand_stack, BinaryOp::Gt)
                .map_err(VmTrap::into_runtime_error)?,
            0x55 => execute_binary(runtime, &mut operand_stack, BinaryOp::Ge)
                .map_err(VmTrap::into_runtime_error)?,
            0x60 => {
                let type_idx =
                    read_u32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
                let size = sizeof_type_from_table(&module.types, type_idx)
                    .map_err(|err| VmTrap::Runtime(err).into_runtime_error())?;
                let size = i32::try_from(size)
                    .map_err(|_| VmTrap::Runtime(RuntimeError::Overflow).into_runtime_error())?;
                operand_stack
                    .push(Value::DInt(size))
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x61 => {
                let value = operand_stack.pop().map_err(VmTrap::into_runtime_error)?;
                let size = size_of_value(runtime.registry(), &value)
                    .map_err(sizeof_error_to_runtime)
                    .map_err(|err| VmTrap::Runtime(err).into_runtime_error())?;
                let size = i32::try_from(size)
                    .map_err(|_| VmTrap::Runtime(RuntimeError::Overflow).into_runtime_error())?;
                operand_stack
                    .push(Value::DInt(size))
                    .map_err(VmTrap::into_runtime_error)?;
            }
            0x70 => {
                let _debug_idx =
                    read_u32(&module.code, &mut pc).map_err(VmTrap::into_runtime_error)?;
            }
            _ => return Err(VmTrap::InvalidOpcode(opcode).into_runtime_error()),
        }
    }

    Ok(())
}

fn deadline_exceeded(deadline: Option<Instant>) -> bool {
    match deadline {
        Some(deadline) => Instant::now() >= deadline,
        None => false,
    }
}

fn vm_statement_location(
    runtime: &Runtime,
    module: &VmModule,
    pou_id: u32,
    pc: usize,
) -> Option<crate::debug::SourceLocation> {
    let source = module.debug_map.source_by_pc.get(&(pou_id, pc as u32))?;
    runtime.resolve_vm_debug_location(source.file.as_str(), source.line, source.column)
}
