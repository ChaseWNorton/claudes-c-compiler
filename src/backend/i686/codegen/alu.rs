//! I686Codegen: ALU operations (integer arithmetic, bitwise, shifts).

use crate::ir::reexports::{IrBinOp, Operand, Value};
use crate::common::types::IrType;
use crate::backend::regalloc::PhysReg;
use crate::emit;
use super::emit::{I686Codegen, alu_mnemonic, shift_mnemonic, phys_reg_name, phys_reg_to_cache_idx};

impl I686Codegen {
    pub(super) fn emit_float_neg_impl(&mut self, ty: IrType) {
        if ty == IrType::F32 {
            self.state.emit("    movd %eax, %xmm0");
            self.state.emit("    movl $0x80000000, %ecx");
            self.state.emit("    movd %ecx, %xmm1");
            self.state.emit("    xorps %xmm1, %xmm0");
            self.state.emit("    movd %xmm0, %eax");
        } else {
            self.state.emit("    xorl $0x80000000, %eax");
        }
    }

    pub(super) fn emit_int_neg_impl(&mut self, _ty: IrType) {
        self.state.emit("    negl %eax");
    }

    pub(super) fn emit_int_not_impl(&mut self, _ty: IrType) {
        self.state.emit("    notl %eax");
    }

    pub(super) fn emit_int_clz_impl(&mut self, ty: IrType) {
        if matches!(ty, IrType::I32 | IrType::U32 | IrType::Ptr) {
            self.state.emit("    lzcntl %eax, %eax");
        } else if matches!(ty, IrType::I16 | IrType::U16) {
            self.state.emit("    lzcntw %ax, %ax");
        } else {
            self.state.emit("    lzcntl %eax, %eax");
        }
    }

    pub(super) fn emit_int_ctz_impl(&mut self, _ty: IrType) {
        self.state.emit("    tzcntl %eax, %eax");
    }

    pub(super) fn emit_int_bswap_impl(&mut self, ty: IrType) {
        match ty {
            IrType::I16 | IrType::U16 => self.state.emit("    rolw $8, %ax"),
            IrType::I32 | IrType::U32 | IrType::Ptr => self.state.emit("    bswapl %eax"),
            _ => self.state.emit("    bswapl %eax"),
        }
    }

    pub(super) fn emit_int_popcount_impl(&mut self, _ty: IrType) {
        self.state.emit("    popcntl %eax, %eax");
    }

    pub(super) fn emit_int_binop_impl(&mut self, dest: &Value, op: IrBinOp, lhs: &Operand, rhs: &Operand, _ty: IrType) {
        let prev_tag = if self.cost_map { Some(self.set_cost_tag("COMPUTE")) } else { None };

        // Register-direct path: when dest has a physical register, operate directly in it
        if let Some(dest_phys) = self.dest_reg(dest) {
            let is_simple_alu = matches!(op, IrBinOp::Add | IrBinOp::Sub | IrBinOp::And
                | IrBinOp::Or | IrBinOp::Xor | IrBinOp::Mul);
            if is_simple_alu {
                self.emit_alu_reg_direct(dest, op, lhs, rhs, dest_phys);
                if let Some(prev) = prev_tag { self.restore_cost_tag(prev); }
                return;
            }
            if matches!(op, IrBinOp::Shl | IrBinOp::AShr | IrBinOp::LShr) {
                self.emit_shift_reg_direct(dest, op, lhs, rhs, dest_phys);
                if let Some(prev) = prev_tag { self.restore_cost_tag(prev); }
                return;
            }
            // Division: ISA requires edx:eax — fall through to accumulator
        }

        // Accumulator path (dest on stack, or division)

        // Immediate optimization for ALU ops
        if matches!(op, IrBinOp::Add | IrBinOp::Sub | IrBinOp::And | IrBinOp::Or | IrBinOp::Xor) {
            if let Some(imm) = Self::const_as_imm32(rhs) {
                self.operand_to_eax(lhs);
                let mnem = alu_mnemonic(op);
                emit!(self.state, "    {}l ${}, %eax", mnem, imm);
                self.state.reg_cache.invalidate_acc();
                if let Some(prev) = prev_tag { self.restore_cost_tag(prev); }
                self.store_eax_to(dest);
                return;
            }
        }

        // Immediate multiply
        if op == IrBinOp::Mul {
            if let Some(imm) = Self::const_as_imm32(rhs) {
                self.operand_to_eax(lhs);
                match imm {
                    3 => emit!(self.state, "    leal (%eax, %eax, 2), %eax"),
                    5 => emit!(self.state, "    leal (%eax, %eax, 4), %eax"),
                    9 => emit!(self.state, "    leal (%eax, %eax, 8), %eax"),
                    _ => emit!(self.state, "    imull ${}, %eax, %eax", imm),
                }
                self.state.reg_cache.invalidate_acc();
                if let Some(prev) = prev_tag { self.restore_cost_tag(prev); }
                self.store_eax_to(dest);
                return;
            }
        }

        // Immediate shift
        if matches!(op, IrBinOp::Shl | IrBinOp::AShr | IrBinOp::LShr) {
            if let Some(imm) = Self::const_as_imm32(rhs) {
                self.operand_to_eax(lhs);
                let mnem = shift_mnemonic(op);
                let shift_amount = (imm as u32) & 31;
                emit!(self.state, "    {} ${}, %eax", mnem, shift_amount);
                self.state.reg_cache.invalidate_acc();
                if let Some(prev) = prev_tag { self.restore_cost_tag(prev); }
                self.store_eax_to(dest);
                return;
            }
        }

        // Direct-operand path: use register/memory source directly
        if matches!(op, IrBinOp::Add | IrBinOp::Sub | IrBinOp::Mul
                      | IrBinOp::And | IrBinOp::Or | IrBinOp::Xor) {
            if let Some(rhs_str) = self.rhs_operand_str(rhs) {
                self.operand_to_eax(lhs);
                match op {
                    IrBinOp::Mul => {
                        if rhs_str.starts_with('$') {
                            emit!(self.state, "    imull {}, %eax, %eax", rhs_str);
                        } else {
                            emit!(self.state, "    imull {}, %eax", rhs_str);
                        }
                    }
                    _ => {
                        let mnem = alu_mnemonic(op);
                        emit!(self.state, "    {}l {}, %eax", mnem, rhs_str);
                    }
                }
                self.state.reg_cache.invalidate_acc();
                if let Some(prev) = prev_tag { self.restore_cost_tag(prev); }
                self.store_eax_to(dest);
                return;
            }
        }

        // Fallback: load lhs to eax, rhs to ecx
        self.operand_to_eax(lhs);
        self.operand_to_ecx(rhs);

        match op {
            IrBinOp::Add => self.state.emit("    addl %ecx, %eax"),
            IrBinOp::Sub => self.state.emit("    subl %ecx, %eax"),
            IrBinOp::Mul => self.state.emit("    imull %ecx, %eax"),
            IrBinOp::And => self.state.emit("    andl %ecx, %eax"),
            IrBinOp::Or => self.state.emit("    orl %ecx, %eax"),
            IrBinOp::Xor => self.state.emit("    xorl %ecx, %eax"),
            IrBinOp::Shl => self.state.emit("    shll %cl, %eax"),
            IrBinOp::AShr => self.state.emit("    sarl %cl, %eax"),
            IrBinOp::LShr => self.state.emit("    shrl %cl, %eax"),
            IrBinOp::SDiv => {
                self.state.emit("    cltd");
                self.state.emit("    idivl %ecx");
            }
            IrBinOp::UDiv => {
                self.state.emit("    xorl %edx, %edx");
                self.state.emit("    divl %ecx");
            }
            IrBinOp::SRem => {
                self.state.emit("    cltd");
                self.state.emit("    idivl %ecx");
                self.state.emit("    movl %edx, %eax");
            }
            IrBinOp::URem => {
                self.state.emit("    xorl %edx, %edx");
                self.state.emit("    divl %ecx");
                self.state.emit("    movl %edx, %eax");
            }
        }
        self.state.reg_cache.invalidate_acc();
        if let Some(prev) = prev_tag { self.restore_cost_tag(prev); }
        self.store_eax_to(dest);
    }

    /// Register-direct ALU for simple ops (add/sub/and/or/xor/mul).
    /// Operates directly in the destination register, avoiding the eax round-trip.
    fn emit_alu_reg_direct(&mut self, dest: &Value, op: IrBinOp, lhs: &Operand,
                           rhs: &Operand, dest_phys: PhysReg) {
        let dest_name = phys_reg_name(dest_phys);

        // Immediate form
        if let Some(imm) = Self::const_as_imm32(rhs) {
            self.operand_to_reg(lhs, dest_phys);
            if op == IrBinOp::Mul {
                match imm {
                    3 => emit!(self.state, "    leal (%{0}, %{0}, 2), %{0}", dest_name),
                    5 => emit!(self.state, "    leal (%{0}, %{0}, 4), %{0}", dest_name),
                    9 => emit!(self.state, "    leal (%{0}, %{0}, 8), %{0}", dest_name),
                    _ => emit!(self.state, "    imull ${}, %{}, %{}", imm, dest_name, dest_name),
                }
            } else {
                let mnem = alu_mnemonic(op);
                emit!(self.state, "    {}l ${}, %{}", mnem, imm, dest_name);
            }
            // eax not touched — cache stays valid
            if let Some(idx) = phys_reg_to_cache_idx(dest_phys) {
                self.state.reg_cache.set_reg(idx, dest.0, false);
            }
            return;
        }

        // Register/memory operand form
        let rhs_phys = self.operand_reg(rhs);
        let rhs_conflicts = rhs_phys.is_some_and(|r| r.0 == dest_phys.0);

        if rhs_conflicts {
            // rhs is in dest register — use eax as scratch
            self.operand_to_eax(rhs);
            self.operand_to_reg(lhs, dest_phys);
            if op == IrBinOp::Mul {
                emit!(self.state, "    imull %eax, %{}", dest_name);
            } else {
                let mnem = alu_mnemonic(op);
                emit!(self.state, "    {}l %eax, %{}", mnem, dest_name);
            }
            self.state.reg_cache.invalidate_acc(); // eax was clobbered
        } else {
            self.operand_to_reg(lhs, dest_phys);
            if let Some(rhs_str) = self.rhs_operand_str(rhs) {
                if op == IrBinOp::Mul {
                    if rhs_str.starts_with('$') {
                        emit!(self.state, "    imull {}, %{}, %{}", rhs_str, dest_name, dest_name);
                    } else {
                        emit!(self.state, "    imull {}, %{}", rhs_str, dest_name);
                    }
                } else {
                    let mnem = alu_mnemonic(op);
                    emit!(self.state, "    {}l {}, %{}", mnem, rhs_str, dest_name);
                }
                // eax not touched — cache stays valid
            } else {
                // rhs has no direct representation — load to eax as scratch
                self.operand_to_eax(rhs);
                if op == IrBinOp::Mul {
                    emit!(self.state, "    imull %eax, %{}", dest_name);
                } else {
                    let mnem = alu_mnemonic(op);
                    emit!(self.state, "    {}l %eax, %{}", mnem, dest_name);
                }
                self.state.reg_cache.invalidate_acc(); // eax was clobbered
            }
        }

        // Update dest register in cache
        if let Some(idx) = phys_reg_to_cache_idx(dest_phys) {
            self.state.reg_cache.set_reg(idx, dest.0, false);
        }
    }

    /// Register-direct shift operations. ISA requires shift count in %cl.
    fn emit_shift_reg_direct(&mut self, dest: &Value, op: IrBinOp, lhs: &Operand,
                             rhs: &Operand, dest_phys: PhysReg) {
        let dest_name = phys_reg_name(dest_phys);
        let mnem = shift_mnemonic(op);

        // Immediate shift
        if let Some(imm) = Self::const_as_imm32(rhs) {
            self.operand_to_reg(lhs, dest_phys);
            let shift_amount = (imm as u32) & 31;
            emit!(self.state, "    {} ${}, %{}", mnem, shift_amount, dest_name);
            // eax not touched — cache stays valid
            if let Some(idx) = phys_reg_to_cache_idx(dest_phys) {
                self.state.reg_cache.set_reg(idx, dest.0, false);
            }
            return;
        }

        // Variable shift — ISA requires %cl
        let rhs_conflicts = self.operand_reg(rhs).is_some_and(|r| r.0 == dest_phys.0);
        if rhs_conflicts {
            // rhs is in dest register — load ecx first, then lhs
            self.operand_to_ecx(rhs);
            self.operand_to_reg(lhs, dest_phys);
        } else {
            self.operand_to_reg(lhs, dest_phys);
            self.operand_to_ecx(rhs);
        }
        emit!(self.state, "    {} %cl, %{}", mnem, dest_name);

        // ecx was used for shift count — invalidate its cache
        self.state.reg_cache.invalidate_reg(4);
        // Update dest register in cache
        if let Some(idx) = phys_reg_to_cache_idx(dest_phys) {
            self.state.reg_cache.set_reg(idx, dest.0, false);
        }
    }
}
