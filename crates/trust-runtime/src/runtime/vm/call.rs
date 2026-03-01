use crate::memory::InstanceId;
use crate::value::Value;

use super::errors::VmTrap;
use super::frames::{FrameStack, VmFrame};
use super::VmModule;

pub(super) fn push_call_frame(
    frame_stack: &mut FrameStack,
    module: &VmModule,
    pou_id: u32,
    return_pc: usize,
    runtime_instance: Option<InstanceId>,
) -> Result<usize, VmTrap> {
    let pou = module.pou(pou_id).ok_or(VmTrap::MissingPou(pou_id))?;
    let local_count = pou.local_ref_count as usize;
    let frame = VmFrame {
        pou_id,
        return_pc,
        code_start: pou.code_start,
        code_end: pou.code_end,
        local_ref_start: pou.local_ref_start,
        local_ref_count: pou.local_ref_count,
        locals: vec![Value::Null; local_count],
        runtime_instance,
        instance_owner: pou.primary_instance_owner,
    };
    let entry_pc = frame.code_start;
    frame_stack.push(frame)?;
    Ok(entry_pc)
}
