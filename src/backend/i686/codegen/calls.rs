//! I686Codegen: function call operations (cdecl calling convention).

use crate::ir::reexports::{Operand, Value, IrConst};
use crate::common::types::IrType;
use crate::backend::call_abi;
use crate::emit;
use crate::backend::traits::ArchCodegen;
use super::emit::I686Codegen;
use crate::backend::generation::is_i128_type;

impl I686Codegen {
    pub(super) fn call_abi_config_impl(&self) -> call_abi::CallAbiConfig {
        call_abi::CallAbiConfig {
            max_int_regs: self.regparm as usize,
            max_float_regs: 0,
            align_i128_pairs: false,
            f128_in_fp_regs: false,
            f128_in_gp_pairs: false,
            variadic_floats_in_gp: false,
            large_struct_by_ref: false,
            use_sysv_struct_classification: false,
            use_riscv_float_struct_classification: false,
            allow_struct_split_reg_stack: false,
            align_struct_pairs: false,
            sret_uses_dedicated_reg: false,
        }
    }

    pub(super) fn emit_call_compute_stack_space_impl(&self, arg_classes: &[call_abi::CallArgClass], arg_types: &[IrType]) -> usize {
        let mut total = 0;
        for (i, ac) in arg_classes.iter().enumerate() {
            let ty = if i < arg_types.len() { arg_types[i] } else { IrType::I32 };
            match ac {
                call_abi::CallArgClass::Stack => {
                    match ty {
                        IrType::F64 | IrType::I64 | IrType::U64 => total += 8,
                        _ => total += 4,
                    }
                }
                call_abi::CallArgClass::F128Stack => total += 12,
                call_abi::CallArgClass::I128Stack => total += 16,
                call_abi::CallArgClass::StructByValStack { size } => total += (*size + 3) & !3,
                call_abi::CallArgClass::LargeStructStack { size } => total += (*size + 3) & !3,
                call_abi::CallArgClass::ZeroSizeSkip => {}
                call_abi::CallArgClass::IntReg { .. } => {} // regparm: in register, no stack space
                _ => total += 4,
            }
        }
        if self.optimize_size {
            (total + 3) & !3
        } else {
            (total + 15) & !15
        }
    }

    pub(super) fn emit_call_f128_pre_convert_impl(&mut self, _args: &[Operand], _arg_classes: &[call_abi::CallArgClass], _arg_types: &[IrType], _stack_arg_space: usize) -> usize {
        0 // No F128 pre-conversion needed on i686
    }

    pub(super) fn emit_call_stack_args_impl(&mut self, args: &[Operand], arg_classes: &[call_abi::CallArgClass],
                            arg_types: &[IrType], stack_arg_space: usize,
                            _fptr_spill: usize, _f128_temp_space: usize) -> i64 {
        // At -Os, use push-based argument passing when all stack args are simple
        // (4-byte or 8-byte). This saves ~8 bytes per 2-arg call vs subl+movl.
        if self.optimize_size && stack_arg_space > 0 {
            let all_pushable = arg_classes.iter().all(|ac| matches!(ac,
                call_abi::CallArgClass::Stack
                | call_abi::CallArgClass::ZeroSizeSkip
                | call_abi::CallArgClass::IntReg { .. }
            ));
            if all_pushable {
                return self.emit_call_push_based_args(args, arg_classes, arg_types, stack_arg_space);
            }
        }

        // Standard subl+movl path
        if stack_arg_space > 0 {
            emit!(self.state, "    subl ${}, %esp", stack_arg_space);
            self.esp_adjust += stack_arg_space as i64;
        }

        let mut stack_offset: usize = 0;
        for (i, ac) in arg_classes.iter().enumerate() {
            match ac {
                call_abi::CallArgClass::I128Stack => {
                    self.emit_call_i128_stack_arg(&args[i], stack_offset);
                    stack_offset += 16;
                }
                call_abi::CallArgClass::F128Stack => {
                    self.emit_call_f128_stack_arg(&args[i], stack_offset);
                    stack_offset += 12;
                }
                call_abi::CallArgClass::StructByValStack { size } |
                call_abi::CallArgClass::LargeStructStack { size } => {
                    let sz = *size;
                    self.emit_call_struct_stack_arg(&args[i], stack_offset, sz);
                    stack_offset += (sz + 3) & !3;
                }
                call_abi::CallArgClass::Stack => {
                    let ty = arg_types[i];
                    if ty == IrType::F64 || ty == IrType::I64 || ty == IrType::U64 {
                        self.emit_call_8byte_stack_arg(&args[i], ty, stack_offset);
                        stack_offset += 8;
                    } else {
                        self.operand_to_eax(&args[i]);
                        emit!(self.state, "    movl %eax, {}(%esp)", stack_offset);
                        stack_offset += 4;
                    }
                }
                call_abi::CallArgClass::ZeroSizeSkip => {}
                call_abi::CallArgClass::IntReg { .. } => {} // regparm: handled in emit_call_reg_args
                _ => {
                    self.operand_to_eax(&args[i]);
                    emit!(self.state, "    movl %eax, {}(%esp)", stack_offset);
                    stack_offset += 4;
                }
            }
        }

        stack_arg_space as i64
    }

    /// Push-based argument passing for -Os. Emits `pushl` instructions in reverse
    /// order instead of `subl $N,%esp` + `movl` stores. Saves ~8 bytes per 2-arg call.
    fn emit_call_push_based_args(&mut self, args: &[Operand], arg_classes: &[call_abi::CallArgClass],
                                  arg_types: &[IrType], stack_arg_space: usize) -> i64 {
        // Collect indices of stack args (skip ZeroSizeSkip and IntReg)
        let mut stack_arg_indices: Vec<usize> = Vec::new();
        for (i, ac) in arg_classes.iter().enumerate() {
            if matches!(ac, call_abi::CallArgClass::Stack) {
                stack_arg_indices.push(i);
            }
        }

        // Push in reverse order (last arg first — cdecl right-to-left)
        for &i in stack_arg_indices.iter().rev() {
            let ty = arg_types[i];
            if ty == IrType::F64 || ty == IrType::I64 || ty == IrType::U64 {
                self.emit_push_8byte_arg(&args[i], ty);
            } else {
                self.emit_push_4byte_arg(&args[i]);
            }
        }

        stack_arg_space as i64
    }

    /// Push a 4-byte argument. Uses `pushl $imm` for constants (2-5 bytes)
    /// or `pushl slot_ref` for values (avoids movl to eax).
    fn emit_push_4byte_arg(&mut self, arg: &Operand) {
        match arg {
            Operand::Const(c) => {
                let val = match c {
                    IrConst::I32(v) => *v as i64,
                    IrConst::I8(v) => *v as i64,
                    IrConst::I16(v) => *v as i64,
                    IrConst::F32(f) => f.to_bits() as i64,
                    IrConst::Zero => 0,
                    _ => {
                        self.operand_to_eax(arg);
                        self.state.emit("    pushl %eax");
                        self.esp_adjust += 4;
                        return;
                    }
                };
                emit!(self.state, "    pushl ${}", val as i32);
            }
            Operand::Value(v) => {
                if let Some(slot) = self.state.get_slot(v.0) {
                    let sr = self.slot_ref(slot);
                    emit!(self.state, "    pushl {}", sr);
                } else {
                    self.operand_to_eax(arg);
                    self.state.emit("    pushl %eax");
                }
            }
            _ => {
                self.operand_to_eax(arg);
                self.state.emit("    pushl %eax");
            }
        }
        self.esp_adjust += 4;
    }

    /// Push an 8-byte argument (I64, U64, F64). Pushes high word first, then low word.
    fn emit_push_8byte_arg(&mut self, arg: &Operand, ty: IrType) {
        match arg {
            Operand::Value(v) => {
                if let Some(slot) = self.state.get_slot(v.0) {
                    // Push high word first, then low word
                    let sr4 = self.slot_ref_offset(slot, 4);
                    emit!(self.state, "    pushl {}", sr4);
                    self.esp_adjust += 4;
                    let sr0 = self.slot_ref(slot);
                    emit!(self.state, "    pushl {}", sr0);
                    self.esp_adjust += 4;
                } else {
                    // No slot — load to eax and push with zero high word
                    self.operand_to_eax(arg);
                    self.state.emit("    pushl $0");
                    self.esp_adjust += 4;
                    self.state.emit("    pushl %eax");
                    self.esp_adjust += 4;
                }
            }
            Operand::Const(IrConst::F64(f)) => {
                let bits = f.to_bits();
                let lo = (bits & 0xFFFF_FFFF) as u32 as i32;
                let hi = (bits >> 32) as u32 as i32;
                emit!(self.state, "    pushl ${}", hi);
                self.esp_adjust += 4;
                emit!(self.state, "    pushl ${}", lo);
                self.esp_adjust += 4;
            }
            Operand::Const(IrConst::I64(v)) => {
                let lo = (*v as u64 & 0xFFFF_FFFF) as u32 as i32;
                let hi = ((*v as u64) >> 32) as u32 as i32;
                emit!(self.state, "    pushl ${}", hi);
                self.esp_adjust += 4;
                emit!(self.state, "    pushl ${}", lo);
                self.esp_adjust += 4;
            }
            _ => {
                // Fallback: load to eax, push with zero high word
                self.operand_to_eax(arg);
                if ty == IrType::F64 {
                    self.state.emit("    pushl $0");
                } else {
                    self.state.emit("    pushl $0");
                }
                self.esp_adjust += 4;
                self.state.emit("    pushl %eax");
                self.esp_adjust += 4;
            }
        }
    }

    pub(super) fn emit_call_reg_args_impl(&mut self, args: &[Operand], arg_classes: &[call_abi::CallArgClass],
                          _arg_types: &[IrType], _stack_info: (i64, usize, usize),
                          _struct_arg_riscv_float_classes: &[Option<crate::common::types::RiscvFloatClass>]) {
        if self.regparm == 0 {
            return; // cdecl: no register args
        }
        // regparm register order: EAX (reg_idx 0), EDX (reg_idx 1), ECX (reg_idx 2).
        // We must load args into registers in reverse order to avoid clobbering
        // EAX (the accumulator) before we're done using it to load other values.
        // Collect register args first, then emit in reverse order.
        let regparm_regs: &[&str] = &["%eax", "%edx", "%ecx"];
        let mut reg_args: Vec<(usize, usize)> = Vec::new(); // (arg_idx, reg_idx)
        for (i, ac) in arg_classes.iter().enumerate() {
            if let call_abi::CallArgClass::IntReg { reg_idx } = ac {
                reg_args.push((i, *reg_idx));
            }
        }
        // Emit in reverse order so we load into edx/ecx before eax
        // (since operand_to_eax uses eax as accumulator).
        for &(arg_i, reg_idx) in reg_args.iter().rev() {
            if reg_idx < regparm_regs.len() {
                let dest_reg = regparm_regs[reg_idx];
                if dest_reg == "%eax" {
                    self.operand_to_eax(&args[arg_i]);
                } else {
                    self.operand_to_eax(&args[arg_i]);
                    emit!(self.state, "    movl %eax, {}", dest_reg);
                    self.state.reg_cache.invalidate_acc();
                }
            }
        }
    }

    pub(super) fn emit_call_instruction_impl(&mut self, direct_name: Option<&str>, func_ptr: Option<&Operand>,
                             indirect: bool, _stack_arg_space: usize) {
        if let Some(name) = direct_name {
            if self.state.needs_plt(name) {
                emit!(self.state, "    call {}@PLT", name);
            } else {
                emit!(self.state, "    call {}", name);
            }
        } else if indirect {
            if let Some(fptr) = func_ptr {
                self.operand_to_eax(fptr);
            }
            self.state.emit("    call *%eax");
        }
    }

    pub(super) fn emit_call_cleanup_impl(&mut self, stack_arg_space: usize, _f128_temp_space: usize, _indirect: bool) {
        if stack_arg_space > 0 {
            emit!(self.state, "    addl ${}, %esp", stack_arg_space);
            self.esp_adjust -= stack_arg_space as i64;
        }
    }

    pub(super) fn emit_call_store_result_impl(&mut self, dest: &Value, return_type: IrType) {
        if return_type == IrType::I64 || return_type == IrType::U64 {
            if let Some(slot) = self.state.get_slot(dest.0) {
                let sr0 = self.slot_ref(slot);
                let sr4 = self.slot_ref_offset(slot, 4);
                emit!(self.state, "    movl %eax, {}", sr0);
                emit!(self.state, "    movl %edx, {}", sr4);
            }
            self.state.reg_cache.invalidate_acc();
        } else if is_i128_type(return_type) {
            self.emit_call_store_i128_result(dest);
        } else if return_type.is_long_double() {
            self.emit_call_store_f128_result(dest);
        } else if return_type == IrType::F32 {
            self.emit_call_move_f32_to_acc();
            self.emit_store_result(dest);
        } else if return_type == IrType::F64 {
            self.emit_f64_store_from_x87(dest);
            self.state.reg_cache.invalidate_acc();
        } else {
            self.emit_store_result(dest);
        }
    }

    pub(super) fn emit_call_store_i128_result_impl(&mut self, dest: &Value) {
        if let Some(slot) = self.state.get_slot(dest.0) {
            let sr0 = self.slot_ref(slot);
            let sr4 = self.slot_ref_offset(slot, 4);
            emit!(self.state, "    movl %eax, {}", sr0);
            emit!(self.state, "    movl %edx, {}", sr4);
        }
    }

    pub(super) fn emit_call_store_f128_result_impl(&mut self, dest: &Value) {
        if let Some(slot) = self.state.get_slot(dest.0) {
            let sr = self.slot_ref(slot);
            emit!(self.state, "    fstpt {}", sr);
            self.state.f128_direct_slots.insert(dest.0);
        }
    }

    pub(super) fn emit_call_move_f32_to_acc_impl(&mut self) {
        self.state.emit("    subl $4, %esp");
        self.state.emit("    fstps (%esp)");
        self.state.emit("    movl (%esp), %eax");
        self.state.emit("    addl $4, %esp");
    }

    pub(super) fn emit_call_move_f64_to_acc_impl(&mut self) {
        self.state.emit("    subl $8, %esp");
        self.state.emit("    fstpl (%esp)");
        self.state.emit("    movl (%esp), %eax");
        self.state.emit("    addl $8, %esp");
    }
}
