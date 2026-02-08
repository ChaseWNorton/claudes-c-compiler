//! x86 instruction emulator for the superoptimizer.
//!
//! Executes instruction sequences against a CpuState. Flag semantics must
//! match real x86 exactly — incorrect flag behavior produces wrong rewrite rules.

use super::ir::*;
use super::cpu::CpuState;

#[derive(Debug)]
pub enum EmulatorError {
    /// Division by zero.
    DivByZero,
    /// Stack access out of bounds.
    StackOutOfBounds,
    /// Unimplemented mnemonic.
    Unimplemented(Mnemonic),
    /// Invalid operand combination.
    InvalidOperands,
}

/// Execute a program against a CPU state, modifying it in place.
pub fn execute(state: &mut CpuState, program: &super::ir::Program) -> Result<(), EmulatorError> {
    for instr in &program.instrs {
        execute_one(state, instr)?;
    }
    Ok(())
}

/// Execute a single instruction.
fn execute_one(state: &mut CpuState, instr: &SInstr) -> Result<(), EmulatorError> {
    match instr.mnemonic {
        Mnemonic::Movl => {
            let val = read_op32(state, &instr.operands[0])?;
            write_op32(state, &instr.operands[1], val)?;
            // movl does NOT affect flags.
        }
        Mnemonic::Addl => {
            let src = read_op32(state, &instr.operands[0])?;
            let dst = read_op32(state, &instr.operands[1])?;
            let (result, carry) = dst.overflowing_add(src);
            write_op32(state, &instr.operands[1], result)?;
            set_flags_add(state, dst, src, result, carry);
        }
        Mnemonic::Subl => {
            let src = read_op32(state, &instr.operands[0])?;
            let dst = read_op32(state, &instr.operands[1])?;
            let (result, borrow) = dst.overflowing_sub(src);
            write_op32(state, &instr.operands[1], result)?;
            set_flags_sub(state, dst, src, result, borrow);
        }
        Mnemonic::Xorl => {
            let src = read_op32(state, &instr.operands[0])?;
            let dst = read_op32(state, &instr.operands[1])?;
            let result = dst ^ src;
            write_op32(state, &instr.operands[1], result)?;
            set_flags_logic(state, result);
        }
        Mnemonic::Andl => {
            let src = read_op32(state, &instr.operands[0])?;
            let dst = read_op32(state, &instr.operands[1])?;
            let result = dst & src;
            write_op32(state, &instr.operands[1], result)?;
            set_flags_logic(state, result);
        }
        Mnemonic::Orl => {
            let src = read_op32(state, &instr.operands[0])?;
            let dst = read_op32(state, &instr.operands[1])?;
            let result = dst | src;
            write_op32(state, &instr.operands[1], result)?;
            set_flags_logic(state, result);
        }
        Mnemonic::Cmpl => {
            let src = read_op32(state, &instr.operands[0])?;
            let dst = read_op32(state, &instr.operands[1])?;
            let (result, borrow) = dst.overflowing_sub(src);
            // cmpl sets flags but does NOT write the result.
            set_flags_sub(state, dst, src, result, borrow);
        }
        Mnemonic::Testl => {
            let src = read_op32(state, &instr.operands[0])?;
            let dst = read_op32(state, &instr.operands[1])?;
            let result = dst & src;
            // testl sets flags but does NOT write the result.
            set_flags_logic(state, result);
        }
        Mnemonic::Negl => {
            let val = read_op32(state, &instr.operands[0])?;
            let result = (val as i32).wrapping_neg() as u32;
            write_op32(state, &instr.operands[0], result)?;
            // negl sets CF=1 if operand != 0, CF=0 if operand == 0.
            state.cf = val != 0;
            state.zf = result == 0;
            state.sf = (result as i32) < 0;
            state.of = val == 0x80000000; // negating MIN_INT overflows
            state.pf = parity(result as u8);
        }
        Mnemonic::Notl => {
            let val = read_op32(state, &instr.operands[0])?;
            let result = !val;
            write_op32(state, &instr.operands[0], result)?;
            // notl does NOT affect any flags.
        }
        Mnemonic::Incl => {
            let val = read_op32(state, &instr.operands[0])?;
            let result = val.wrapping_add(1);
            write_op32(state, &instr.operands[0], result)?;
            // incl sets OF, SF, ZF, PF but NOT CF.
            state.zf = result == 0;
            state.sf = (result as i32) < 0;
            state.of = val == 0x7FFFFFFF; // signed overflow: MAX_INT + 1
            state.pf = parity(result as u8);
            // CF is UNCHANGED.
        }
        Mnemonic::Decl => {
            let val = read_op32(state, &instr.operands[0])?;
            let result = val.wrapping_sub(1);
            write_op32(state, &instr.operands[0], result)?;
            // decl sets OF, SF, ZF, PF but NOT CF.
            state.zf = result == 0;
            state.sf = (result as i32) < 0;
            state.of = val == 0x80000000; // signed overflow: MIN_INT - 1
            state.pf = parity(result as u8);
            // CF is UNCHANGED.
        }
        Mnemonic::Shll => {
            let count = read_shift_count(state, &instr.operands[0])?;
            let val = read_op32(state, &instr.operands[1])?;
            if count == 0 {
                // Shift by 0: no flags changed.
            } else {
                let count = count & 31; // x86 masks to 5 bits
                let result = val.wrapping_shl(count);
                write_op32(state, &instr.operands[1], result)?;
                // CF = last bit shifted out
                state.cf = count <= 32 && (val >> (32 - count)) & 1 != 0;
                state.zf = result == 0;
                state.sf = (result as i32) < 0;
                state.pf = parity(result as u8);
                if count == 1 {
                    // OF = XOR of CF and MSB of result
                    state.of = state.cf ^ state.sf;
                }
                // OF is undefined for count > 1
            }
        }
        Mnemonic::Shrl => {
            let count = read_shift_count(state, &instr.operands[0])?;
            let val = read_op32(state, &instr.operands[1])?;
            if count == 0 {
                // No flags changed.
            } else {
                let count = count & 31;
                let result = val.wrapping_shr(count);
                write_op32(state, &instr.operands[1], result)?;
                // CF = last bit shifted out
                state.cf = (val >> (count - 1)) & 1 != 0;
                state.zf = result == 0;
                state.sf = (result as i32) < 0;
                state.pf = parity(result as u8);
                if count == 1 {
                    state.of = (val as i32) < 0; // OF = MSB of original value
                }
            }
        }
        Mnemonic::Sarl => {
            let count = read_shift_count(state, &instr.operands[0])?;
            let val = read_op32(state, &instr.operands[1])?;
            if count == 0 {
                // No flags changed.
            } else {
                let count = count & 31;
                let result = ((val as i32).wrapping_shr(count)) as u32;
                write_op32(state, &instr.operands[1], result)?;
                state.cf = (val >> (count - 1)) & 1 != 0;
                state.zf = result == 0;
                state.sf = (result as i32) < 0;
                state.pf = parity(result as u8);
                if count == 1 {
                    state.of = false; // SAR by 1 always clears OF
                }
            }
        }
        Mnemonic::Leal => {
            // LEA computes an address but does NOT dereference memory.
            // Does NOT affect flags.
            let addr = compute_lea_address(state, &instr.operands[0])?;
            write_op32(state, &instr.operands[1], addr)?;
        }
        Mnemonic::Imull => {
            // Two-operand form: imull src, dst → dst = dst * src (low 32 bits).
            let src = read_op32(state, &instr.operands[0])?;
            let dst = read_op32(state, &instr.operands[1])?;
            let result_wide = (dst as i32 as i64) * (src as i32 as i64);
            let result = result_wide as u32;
            write_op32(state, &instr.operands[1], result)?;
            // CF = OF = 1 if sign-extended low 32 bits != full 64-bit result
            let overflow = result_wide != (result as i32 as i64);
            state.cf = overflow;
            state.of = overflow;
            // SF, ZF, PF are undefined per Intel manual, but real CPUs set them.
            // We leave them for safety.
            state.zf = result == 0;
            state.sf = (result as i32) < 0;
            state.pf = parity(result as u8);
        }
        Mnemonic::Movzbl => {
            // Zero-extend byte to long.
            let val = read_op8(state, &instr.operands[0])?;
            write_op32(state, &instr.operands[1], val as u32)?;
            // Does NOT affect flags.
        }
        Mnemonic::Movsbl => {
            // Sign-extend byte to long.
            let val = read_op8(state, &instr.operands[0])?;
            write_op32(state, &instr.operands[1], val as i8 as i32 as u32)?;
            // Does NOT affect flags.
        }
        Mnemonic::Cltd => {
            // Sign-extend eax into edx:eax. edx = (eax >> 31) ? 0xFFFFFFFF : 0.
            let eax = state.regs[REG_EAX as usize];
            state.regs[REG_EDX as usize] = if (eax as i32) < 0 { 0xFFFFFFFF } else { 0 };
            // Does NOT affect flags.
        }
        Mnemonic::Xchgl => {
            // Exchange two operands. Does NOT affect flags.
            let a = read_op32(state, &instr.operands[0])?;
            let b = read_op32(state, &instr.operands[1])?;
            write_op32(state, &instr.operands[0], b)?;
            write_op32(state, &instr.operands[1], a)?;
        }
        Mnemonic::Pushl | Mnemonic::Popl => {
            // Push/pop modify ESP and memory. For superopt purposes,
            // we don't model the stack pointer changes — skip these.
            return Err(EmulatorError::Unimplemented(instr.mnemonic));
        }
    }
    Ok(())
}

/// Read a 32-bit value from an operand.
fn read_op32(state: &CpuState, op: &SOperand) -> Result<u32, EmulatorError> {
    match *op {
        SOperand::Reg(r) => Ok(state.regs[r as usize]),
        SOperand::Imm(v) => Ok(v as u32),
        SOperand::MemEbp(off) => {
            state.read_stack32(off).ok_or(EmulatorError::StackOutOfBounds)
        }
        _ => Err(EmulatorError::InvalidOperands),
    }
}

/// Read an 8-bit value from an operand (for movzbl/movsbl).
fn read_op8(state: &CpuState, op: &SOperand) -> Result<u8, EmulatorError> {
    match *op {
        SOperand::Reg(r) => Ok(state.regs[r as usize] as u8), // low byte
        SOperand::MemEbp(off) => {
            state.read_stack8(off).ok_or(EmulatorError::StackOutOfBounds)
        }
        _ => Err(EmulatorError::InvalidOperands),
    }
}

/// Write a 32-bit value to an operand.
fn write_op32(state: &mut CpuState, op: &SOperand, val: u32) -> Result<(), EmulatorError> {
    match *op {
        SOperand::Reg(r) => {
            state.regs[r as usize] = val;
            Ok(())
        }
        SOperand::MemEbp(off) => {
            state.write_stack32(off, val).ok_or(EmulatorError::StackOutOfBounds)
        }
        _ => Err(EmulatorError::InvalidOperands),
    }
}

/// Read a shift count from the first operand (immediate or CL register).
fn read_shift_count(state: &CpuState, op: &SOperand) -> Result<u32, EmulatorError> {
    match *op {
        SOperand::Imm(v) => Ok((v as u32) & 31),
        SOperand::Reg(r) => Ok(state.regs[r as usize] & 31), // Only CL matters on x86
        _ => Err(EmulatorError::InvalidOperands),
    }
}

/// Compute the effective address for a LEA operand.
fn compute_lea_address(state: &CpuState, op: &SOperand) -> Result<u32, EmulatorError> {
    match *op {
        SOperand::MemEbp(off) => {
            Ok(state.regs[REG_EBP as usize].wrapping_add(off as u32))
        }
        SOperand::LeaMem(base, index, scale, disp) => {
            let base_val = state.regs[base as usize];
            let index_val = state.regs[index as usize];
            Ok(base_val
                .wrapping_add(index_val.wrapping_mul(scale as u32))
                .wrapping_add(disp as u32))
        }
        _ => Err(EmulatorError::InvalidOperands),
    }
}

// ── Flag helpers ─────────────────────────────────────────────────────────

fn set_flags_add(state: &mut CpuState, a: u32, b: u32, result: u32, carry: bool) {
    state.cf = carry;
    state.zf = result == 0;
    state.sf = (result as i32) < 0;
    // Signed overflow: both operands same sign, result different sign.
    state.of = ((a ^ result) & (b ^ result) & 0x80000000) != 0;
    state.pf = parity(result as u8);
}

fn set_flags_sub(state: &mut CpuState, a: u32, b: u32, result: u32, borrow: bool) {
    state.cf = borrow;
    state.zf = result == 0;
    state.sf = (result as i32) < 0;
    // Signed overflow for subtraction: operands different sign, result sign != a's sign.
    state.of = ((a ^ b) & (a ^ result) & 0x80000000) != 0;
    state.pf = parity(result as u8);
}

fn set_flags_logic(state: &mut CpuState, result: u32) {
    state.cf = false;
    state.of = false;
    state.zf = result == 0;
    state.sf = (result as i32) < 0;
    state.pf = parity(result as u8);
}

/// Compute even parity of the low byte.
fn parity(b: u8) -> bool {
    b.count_ones() % 2 == 0
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::ir::*;

    fn make_state(eax: u32, ecx: u32, edx: u32) -> CpuState {
        CpuState::new([eax, ecx, edx, 0, 0, 0x1000, 0, 0])
    }

    fn run(state: &mut CpuState, instrs: Vec<SInstr>) -> Result<(), EmulatorError> {
        execute(state, &Program::new(instrs))
    }

    #[test]
    fn test_movl_reg_reg() {
        let mut s = make_state(42, 0, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Movl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_ECX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_ECX as usize], 42);
    }

    #[test]
    fn test_movl_imm_reg() {
        let mut s = make_state(0, 0, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Movl, SOperand::Imm(0xDEADBEEF_u32 as i32), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 0xDEADBEEF);
    }

    #[test]
    fn test_addl_sets_flags() {
        let mut s = make_state(0x7FFFFFFF, 1, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Addl, SOperand::Reg(REG_ECX), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 0x80000000);
        assert!(s.of, "should overflow");
        assert!(s.sf, "should be negative");
        assert!(!s.zf, "should not be zero");
        assert!(!s.cf, "should not carry");
    }

    #[test]
    fn test_addl_carry() {
        let mut s = make_state(0xFFFFFFFF, 1, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Addl, SOperand::Reg(REG_ECX), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 0);
        assert!(s.cf, "should carry");
        assert!(s.zf, "should be zero");
    }

    #[test]
    fn test_subl() {
        let mut s = make_state(10, 3, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Subl, SOperand::Reg(REG_ECX), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 7);
        assert!(!s.cf);
        assert!(!s.zf);
    }

    #[test]
    fn test_subl_borrow() {
        let mut s = make_state(0, 1, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Subl, SOperand::Reg(REG_ECX), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 0xFFFFFFFF);
        assert!(s.cf, "should borrow");
    }

    #[test]
    fn test_xorl_self_clears() {
        let mut s = make_state(42, 0, 0);
        s.cf = true;
        s.of = true;
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Xorl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 0);
        assert!(s.zf);
        assert!(!s.cf, "xor clears CF");
        assert!(!s.of, "xor clears OF");
    }

    #[test]
    fn test_incl_preserves_cf() {
        let mut s = make_state(5, 0, 0);
        s.cf = true; // Set CF before incl.
        run(&mut s, vec![
            SInstr::new1(Mnemonic::Incl, SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 6);
        assert!(s.cf, "incl must NOT modify CF");
    }

    #[test]
    fn test_incl_overflow() {
        let mut s = make_state(0x7FFFFFFF, 0, 0);
        run(&mut s, vec![
            SInstr::new1(Mnemonic::Incl, SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 0x80000000);
        assert!(s.of, "incl MAX_INT should overflow");
        assert!(s.sf, "result should be negative");
    }

    #[test]
    fn test_decl_preserves_cf() {
        let mut s = make_state(5, 0, 0);
        s.cf = true;
        run(&mut s, vec![
            SInstr::new1(Mnemonic::Decl, SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 4);
        assert!(s.cf, "decl must NOT modify CF");
    }

    #[test]
    fn test_negl() {
        let mut s = make_state(5, 0, 0);
        run(&mut s, vec![
            SInstr::new1(Mnemonic::Negl, SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize] as i32, -5);
        assert!(s.cf, "negl nonzero sets CF");
    }

    #[test]
    fn test_negl_zero() {
        let mut s = make_state(0, 0, 0);
        run(&mut s, vec![
            SInstr::new1(Mnemonic::Negl, SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 0);
        assert!(!s.cf, "negl zero clears CF");
        assert!(s.zf);
    }

    #[test]
    fn test_notl_no_flags() {
        let mut s = make_state(0xFF00FF00, 0, 0);
        s.cf = true;
        s.zf = true;
        run(&mut s, vec![
            SInstr::new1(Mnemonic::Notl, SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 0x00FF00FF);
        assert!(s.cf, "notl must not change CF");
        assert!(s.zf, "notl must not change ZF");
    }

    #[test]
    fn test_shll() {
        let mut s = make_state(0x80000001, 0, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Shll, SOperand::Imm(1), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 2);
        assert!(s.cf, "high bit should shift into CF");
    }

    #[test]
    fn test_shrl() {
        let mut s = make_state(3, 0, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Shrl, SOperand::Imm(1), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 1);
        assert!(s.cf, "low bit should shift into CF");
    }

    #[test]
    fn test_sarl_sign_extends() {
        let mut s = make_state(0x80000004, 0, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Sarl, SOperand::Imm(1), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 0xC0000002);
        assert!(!s.cf);
    }

    #[test]
    fn test_leal() {
        let mut s = make_state(10, 20, 0);
        run(&mut s, vec![
            SInstr::new2(
                Mnemonic::Leal,
                SOperand::LeaMem(REG_EAX, REG_ECX, 4, 5),
                SOperand::Reg(REG_EDX),
            ),
        ]).unwrap();
        // 10 + 20*4 + 5 = 95
        assert_eq!(s.regs[REG_EDX as usize], 95);
    }

    #[test]
    fn test_imull() {
        let mut s = make_state(7, 6, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Imull, SOperand::Reg(REG_ECX), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 42);
        assert!(!s.cf);
        assert!(!s.of);
    }

    #[test]
    fn test_imull_overflow() {
        let mut s = make_state(0x7FFFFFFF, 2, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Imull, SOperand::Reg(REG_ECX), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert!(s.cf, "should overflow");
        assert!(s.of, "should overflow");
    }

    #[test]
    fn test_cltd() {
        let mut s = make_state(0x80000000, 0, 0);
        run(&mut s, vec![SInstr::new0(Mnemonic::Cltd)]).unwrap();
        assert_eq!(s.regs[REG_EDX as usize], 0xFFFFFFFF);

        let mut s2 = make_state(0x7FFFFFFF, 0, 0);
        run(&mut s2, vec![SInstr::new0(Mnemonic::Cltd)]).unwrap();
        assert_eq!(s2.regs[REG_EDX as usize], 0);
    }

    #[test]
    fn test_xchgl() {
        let mut s = make_state(10, 20, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Xchgl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_ECX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_EAX as usize], 20);
        assert_eq!(s.regs[REG_ECX as usize], 10);
    }

    #[test]
    fn test_cmpl() {
        let mut s = make_state(5, 5, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Cmpl, SOperand::Reg(REG_ECX), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert!(s.zf, "equal values should set ZF");
        assert!(!s.cf, "equal values should not set CF");
        // EAX should be unchanged.
        assert_eq!(s.regs[REG_EAX as usize], 5);
    }

    #[test]
    fn test_testl() {
        let mut s = make_state(0, 0, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Testl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_EAX)),
        ]).unwrap();
        assert!(s.zf, "zero AND zero should set ZF");
        assert!(!s.cf, "testl clears CF");
        assert!(!s.of, "testl clears OF");
    }

    #[test]
    fn test_movzbl() {
        let mut s = make_state(0xFFFFFF80, 0, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Movzbl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_ECX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_ECX as usize], 0x80); // zero-extended
    }

    #[test]
    fn test_movsbl() {
        let mut s = make_state(0xFFFFFF80, 0, 0);
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Movsbl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_ECX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_ECX as usize], 0xFFFFFF80); // sign-extended
    }

    #[test]
    fn test_memory_roundtrip() {
        let mut s = make_state(42, 0, 0);
        s.regs[REG_EBP as usize] = 0x1000;
        run(&mut s, vec![
            SInstr::new2(Mnemonic::Movl, SOperand::Reg(REG_EAX), SOperand::MemEbp(-8)),
            SInstr::new2(Mnemonic::Movl, SOperand::MemEbp(-8), SOperand::Reg(REG_ECX)),
        ]).unwrap();
        assert_eq!(s.regs[REG_ECX as usize], 42);
    }

    #[test]
    fn test_parity() {
        assert!(parity(0x00)); // 0 ones = even
        assert!(!parity(0x01)); // 1 one = odd
        assert!(parity(0x03)); // 2 ones = even
        assert!(!parity(0x07)); // 3 ones = odd
        assert!(parity(0xFF)); // 8 ones = even
    }
}
