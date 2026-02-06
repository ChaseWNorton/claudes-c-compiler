//! i686 inline assembly template substitution and register formatting.
//!
//! The default register size is 32-bit (eax, etc.). Supports GCC-style modifiers
//! for size variants (w=16-bit, b=8-bit low, h=8-bit high) and special operand
//! forms (c=raw constant, P=raw symbol, a=address, n=negated immediate).
//!
//! Template parsing delegates to the shared `x86_common` module. Only operand
//! emission differs from x86-64 (no RIP-relative addressing, 32-bit default).

use std::borrow::Cow;
use std::fmt::Write;
use crate::common::types::IrType;
use crate::ir::reexports::BlockId;
use crate::backend::x86_common;
use super::emit::I686Codegen;

impl I686Codegen {
    /// Substitute %0, %1, %[name], %k0, %b1, %w2, %h3, %c0, %P0, %a0, %n0, %l[name] etc.
    /// in i686 asm template.
    ///
    /// Delegates to the shared x86 template parser with an i686-specific
    /// operand emission callback.
    pub(super) fn substitute_i686_asm_operands(
        line: &str,
        ops: &x86_common::AsmOperands,
        gcc_to_internal: &[usize],
        goto_labels: &[(String, BlockId)],
    ) -> String {
        x86_common::substitute_x86_asm_operands(
            line, ops, gcc_to_internal, goto_labels,
            Self::emit_i686_operand,
        )
    }

    /// Emit a single operand with the given modifier into the result string.
    fn emit_i686_operand(
        result: &mut String,
        idx: usize,
        modifier: Option<char>,
        ops: &x86_common::AsmOperands,
    ) {
        if x86_common::emit_operand_common(result, idx, modifier, ops) {
            return;
        }

        let has_symbol = ops.op_imm_symbols.get(idx).and_then(|s| s.as_ref());
        let has_imm = ops.op_imm_values.get(idx).and_then(|v| v.as_ref());

        if modifier == Some('a') {
            if let Some(sym) = has_symbol {
                result.push_str(sym);
            } else if let Some(imm) = has_imm {
                result.push_str(&imm.to_string());
            } else if ops.op_is_memory[idx] {
                result.push_str(&ops.op_mem_addrs[idx]);
            } else {
                let _ = write!(result, "(%{})", ops.op_regs[idx]);
            }
        } else {
            let effective_mod = modifier.or_else(|| Self::i686_default_modifier_for_type(ops.op_types.get(idx).copied()));
            result.push('%');
            result.push_str(&Self::format_i686_reg(&ops.op_regs[idx], effective_mod));
        }
    }

    /// Determine the default register size modifier based on the operand's IR type.
    /// On i686, the default is 32-bit, so only smaller types get a modifier.
    fn i686_default_modifier_for_type(ty: Option<IrType>) -> Option<char> {
        match ty {
            Some(IrType::I8) | Some(IrType::U8) => Some('b'),
            Some(IrType::I16) | Some(IrType::U16) => Some('w'),
            // 32-bit is the default on i686
            _ => None,
        }
    }

    /// Format i686 register with size modifier.
    /// On i686, default (no modifier or 'k') is 32-bit.
    fn format_i686_reg<'a>(reg: &'a str, modifier: Option<char>) -> Cow<'a, str> {
        if reg.starts_with("xmm") || reg.starts_with("st(") || reg == "st" {
            return Cow::Borrowed(reg);
        }
        match modifier {
            Some('w') => x86_common::reg_to_16(reg),
            Some('b') => x86_common::reg_to_8l(reg),
            Some('h') => x86_common::reg_to_8h(reg),
            // 'k', 'l', 'q', or no modifier => 32-bit (no 64-bit on i686)
            _ => x86_common::reg_to_32(reg),
        }
    }
}
