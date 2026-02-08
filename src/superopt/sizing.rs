//! Pre-computed instruction byte sizes for the superoptimizer.
//!
//! Avoids invoking the full parser/encoder during search enumeration.
//! Sizes match i686 encoding (no REX prefix, 32-bit default operand size).

use super::ir::*;

/// Compute the byte size of an instruction from its mnemonic and operands.
/// Returns None for invalid operand combinations.
pub fn instruction_size(instr: &SInstr) -> Option<usize> {
    match instr.mnemonic {
        Mnemonic::Movl => size_mov(&instr.operands),
        Mnemonic::Addl | Mnemonic::Subl | Mnemonic::Xorl
        | Mnemonic::Andl | Mnemonic::Orl | Mnemonic::Cmpl => {
            size_alu(&instr.operands, instr.mnemonic)
        }
        Mnemonic::Testl => size_test(&instr.operands),
        Mnemonic::Negl | Mnemonic::Notl => size_unary_f7(&instr.operands),
        Mnemonic::Incl => size_inc_dec(&instr.operands),
        Mnemonic::Decl => size_inc_dec(&instr.operands),
        Mnemonic::Shll | Mnemonic::Shrl | Mnemonic::Sarl => size_shift(&instr.operands),
        Mnemonic::Leal => size_lea(&instr.operands),
        Mnemonic::Imull => size_imul(&instr.operands),
        Mnemonic::Movzbl | Mnemonic::Movsbl => size_movx(&instr.operands),
        Mnemonic::Cltd => Some(1),     // 0x99
        Mnemonic::Pushl => size_push(&instr.operands),
        Mnemonic::Popl => size_pop(&instr.operands),
        Mnemonic::Xchgl => size_xchg(&instr.operands),
    }
}

/// Compute total byte size of a program.
pub fn program_size(program: &Program) -> Option<usize> {
    let mut total = 0;
    for instr in &program.instrs {
        total += instruction_size(instr)?;
    }
    Some(total)
}

// ── MOV sizing ───────────────────────────────────────────────────────────

fn size_mov(ops: &[SOperand; 3]) -> Option<usize> {
    match (&ops[0], &ops[1]) {
        // movl %reg, %reg → 2 bytes (89 ModRM)
        (SOperand::Reg(_), SOperand::Reg(_)) => Some(2),
        // movl $imm, %reg → 5 bytes (B8+r imm32)
        (SOperand::Imm(_), SOperand::Reg(_)) => Some(5),
        // movl $imm, offset(%ebp) → 2 + disp_size + 4
        (SOperand::Imm(_), SOperand::MemEbp(off)) => Some(2 + disp_size(*off) + 4),
        // movl %reg, offset(%ebp) → 2 + disp_size
        (SOperand::Reg(_), SOperand::MemEbp(off)) => Some(2 + disp_size(*off)),
        // movl offset(%ebp), %reg → 2 + disp_size
        (SOperand::MemEbp(off), SOperand::Reg(_)) => Some(2 + disp_size(*off)),
        _ => None,
    }
}

// ── ALU sizing (add, sub, xor, and, or, cmp) ────────────────────────────

fn size_alu(ops: &[SOperand; 3], mnemonic: Mnemonic) -> Option<usize> {
    match (&ops[0], &ops[1]) {
        // alu %reg, %reg → 2 bytes
        (SOperand::Reg(_), SOperand::Reg(_)) => Some(2),
        // alu $imm, %eax → 5 bytes (short form: opcode + imm32) if imm doesn't fit in i8
        // alu $imm, %eax → 3 bytes (83 /x imm8) if fits in i8
        (SOperand::Imm(v), SOperand::Reg(r)) => {
            if fits_i8(*v) {
                Some(3) // 83 ModRM imm8
            } else if *r == REG_EAX {
                Some(5) // short form: opcode + imm32
            } else {
                Some(6) // 81 ModRM imm32
            }
        }
        // alu %reg, offset(%ebp) → 2 + disp_size
        (SOperand::Reg(_), SOperand::MemEbp(off)) => Some(2 + disp_size(*off)),
        // alu offset(%ebp), %reg → 2 + disp_size
        (SOperand::MemEbp(off), SOperand::Reg(_)) => Some(2 + disp_size(*off)),
        // alu $imm, offset(%ebp) → 2 + disp + imm_size
        (SOperand::Imm(v), SOperand::MemEbp(off)) => {
            if fits_i8(*v) {
                Some(2 + disp_size(*off) + 1) // 83 ModRM disp imm8
            } else {
                Some(2 + disp_size(*off) + 4) // 81 ModRM disp imm32
            }
        }
        _ => None,
    }
}

// ── TEST sizing ──────────────────────────────────────────────────────────

fn size_test(ops: &[SOperand; 3]) -> Option<usize> {
    match (&ops[0], &ops[1]) {
        (SOperand::Reg(_), SOperand::Reg(_)) => Some(2),
        (SOperand::Imm(_), SOperand::Reg(REG_EAX)) => Some(5), // A9 imm32
        (SOperand::Imm(_), SOperand::Reg(_)) => Some(6),       // F7 /0 imm32
        _ => None,
    }
}

// ── Unary F7 (neg, not) ─────────────────────────────────────────────────

fn size_unary_f7(ops: &[SOperand; 3]) -> Option<usize> {
    match &ops[0] {
        SOperand::Reg(_) => Some(2),   // F7 ModRM
        SOperand::MemEbp(off) => Some(2 + disp_size(*off)),
        _ => None,
    }
}

// ── INC/DEC sizing (i686 has 1-byte forms!) ─────────────────────────────

fn size_inc_dec(ops: &[SOperand; 3]) -> Option<usize> {
    match &ops[0] {
        // i686 has 1-byte inc/dec: 40+r / 48+r
        SOperand::Reg(_) => Some(1),
        SOperand::MemEbp(off) => Some(2 + disp_size(*off)),
        _ => None,
    }
}

// ── Shift sizing ─────────────────────────────────────────────────────────

fn size_shift(ops: &[SOperand; 3]) -> Option<usize> {
    match (&ops[0], &ops[1]) {
        // shl $1, %reg → 2 bytes (D1 ModRM, short form for shift-by-1)
        (SOperand::Imm(1), SOperand::Reg(_)) => Some(2),
        // shl $imm, %reg → 3 bytes (C1 ModRM imm8)
        (SOperand::Imm(_), SOperand::Reg(_)) => Some(3),
        // shl %cl, %reg → 2 bytes (D3 ModRM)
        (SOperand::Reg(REG_ECX), SOperand::Reg(_)) => Some(2),
        _ => None,
    }
}

// ── LEA sizing ───────────────────────────────────────────────────────────

fn size_lea(ops: &[SOperand; 3]) -> Option<usize> {
    match (&ops[0], &ops[1]) {
        // leal offset(%ebp), %reg → 2 + disp_size
        (SOperand::MemEbp(off), SOperand::Reg(_)) => Some(2 + disp_size(*off)),
        // leal disp(%base, %index, scale), %reg → 3 + disp_size
        (SOperand::LeaMem(_, _, _, disp), SOperand::Reg(_)) => Some(3 + disp_size(*disp)),
        _ => None,
    }
}

// ── IMUL sizing ──────────────────────────────────────────────────────────

fn size_imul(ops: &[SOperand; 3]) -> Option<usize> {
    match (&ops[0], &ops[1]) {
        // imull %reg, %reg → 3 bytes (0F AF ModRM)
        (SOperand::Reg(_), SOperand::Reg(_)) => Some(3),
        // imull offset(%ebp), %reg → 3 + disp_size
        (SOperand::MemEbp(off), SOperand::Reg(_)) => Some(3 + disp_size(*off)),
        _ => None,
    }
}

// ── MOVZX/MOVSX sizing ──────────────────────────────────────────────────

fn size_movx(ops: &[SOperand; 3]) -> Option<usize> {
    match (&ops[0], &ops[1]) {
        // movzbl/movsbl %reg, %reg → 3 bytes (0F B6/BE ModRM)
        (SOperand::Reg(_), SOperand::Reg(_)) => Some(3),
        // movzbl/movsbl offset(%ebp), %reg → 3 + disp_size
        (SOperand::MemEbp(off), SOperand::Reg(_)) => Some(3 + disp_size(*off)),
        _ => None,
    }
}

// ── PUSH/POP sizing ─────────────────────────────────────────────────────

fn size_push(ops: &[SOperand; 3]) -> Option<usize> {
    match &ops[0] {
        SOperand::Reg(_) => Some(1),      // 50+r
        SOperand::Imm(v) if fits_i8(*v) => Some(2),  // 6A imm8
        SOperand::Imm(_) => Some(5),      // 68 imm32
        _ => None,
    }
}

fn size_pop(ops: &[SOperand; 3]) -> Option<usize> {
    match &ops[0] {
        SOperand::Reg(_) => Some(1),      // 58+r
        _ => None,
    }
}

// ── XCHG sizing ──────────────────────────────────────────────────────────

fn size_xchg(ops: &[SOperand; 3]) -> Option<usize> {
    match (&ops[0], &ops[1]) {
        // xchgl %eax, %reg or %reg, %eax → 1 byte (90+r)
        (SOperand::Reg(REG_EAX), SOperand::Reg(_))
        | (SOperand::Reg(_), SOperand::Reg(REG_EAX)) => Some(1),
        // xchgl %reg, %reg → 2 bytes
        (SOperand::Reg(_), SOperand::Reg(_)) => Some(2),
        _ => None,
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Size of an ebp-relative displacement in ModRM encoding.
/// EBP always needs a displacement byte (no 0-disp encoding for EBP).
fn disp_size(offset: i32) -> usize {
    if offset >= -128 && offset <= 127 {
        1 // disp8
    } else {
        4 // disp32
    }
}

/// Whether an immediate value fits in a sign-extended byte.
fn fits_i8(v: i32) -> bool {
    v >= -128 && v <= 127
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_movl_sizes() {
        // movl %eax, %ecx → 2
        assert_eq!(instruction_size(&SInstr::new2(Mnemonic::Movl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_ECX))), Some(2));
        // movl $42, %eax → 5
        assert_eq!(instruction_size(&SInstr::new2(Mnemonic::Movl, SOperand::Imm(42), SOperand::Reg(REG_EAX))), Some(5));
        // movl %eax, -8(%ebp) → 3 (2 + disp8)
        assert_eq!(instruction_size(&SInstr::new2(Mnemonic::Movl, SOperand::Reg(REG_EAX), SOperand::MemEbp(-8))), Some(3));
    }

    #[test]
    fn test_alu_sizes() {
        // addl %ecx, %eax → 2
        assert_eq!(instruction_size(&SInstr::new2(Mnemonic::Addl, SOperand::Reg(REG_ECX), SOperand::Reg(REG_EAX))), Some(2));
        // addl $1, %eax → 3 (fits i8)
        assert_eq!(instruction_size(&SInstr::new2(Mnemonic::Addl, SOperand::Imm(1), SOperand::Reg(REG_EAX))), Some(3));
        // addl $1000, %eax → 5 (eax short form)
        assert_eq!(instruction_size(&SInstr::new2(Mnemonic::Addl, SOperand::Imm(1000), SOperand::Reg(REG_EAX))), Some(5));
        // addl $1000, %ecx → 6 (non-eax, imm32)
        assert_eq!(instruction_size(&SInstr::new2(Mnemonic::Addl, SOperand::Imm(1000), SOperand::Reg(REG_ECX))), Some(6));
    }

    #[test]
    fn test_incl_1byte() {
        // incl %eax → 1 (i686 short form 40+r)
        assert_eq!(instruction_size(&SInstr::new1(Mnemonic::Incl, SOperand::Reg(REG_EAX))), Some(1));
        assert_eq!(instruction_size(&SInstr::new1(Mnemonic::Decl, SOperand::Reg(REG_ECX))), Some(1));
    }

    #[test]
    fn test_xorl_size() {
        // xorl %eax, %eax → 2
        assert_eq!(instruction_size(&SInstr::new2(Mnemonic::Xorl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_EAX))), Some(2));
    }

    #[test]
    fn test_known_size_optimizations() {
        // movl $0, %eax is 5 bytes; xorl %eax, %eax is 2 bytes → saves 3
        let mov0 = instruction_size(&SInstr::new2(Mnemonic::Movl, SOperand::Imm(0), SOperand::Reg(REG_EAX))).unwrap();
        let xor0 = instruction_size(&SInstr::new2(Mnemonic::Xorl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_EAX))).unwrap();
        assert_eq!(mov0 - xor0, 3);

        // addl $1, %eax is 3 bytes; incl %eax is 1 byte → saves 2
        let add1 = instruction_size(&SInstr::new2(Mnemonic::Addl, SOperand::Imm(1), SOperand::Reg(REG_EAX))).unwrap();
        let inc = instruction_size(&SInstr::new1(Mnemonic::Incl, SOperand::Reg(REG_EAX))).unwrap();
        assert_eq!(add1 - inc, 2);
    }
}
