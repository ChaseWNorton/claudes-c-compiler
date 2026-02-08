//! Equivalence verification for the superoptimizer.
//!
//! Tests whether two instruction sequences produce the same output state
//! across many input vectors (edge cases + random).

use super::ir::Program;
use super::cpu::{CpuState, OutputMask, Rng};
use super::emulator::execute;

/// Number of random test vectors for full verification.
pub const DEFAULT_RANDOM_VECTORS: usize = 2000;

/// Number of random test vectors for quick screening.
pub const QUICK_RANDOM_VECTORS: usize = 32;

/// Hand-picked edge case register vectors.
pub fn edge_case_vectors() -> Vec<[u32; 8]> {
    vec![
        // All zeros.
        [0, 0, 0, 0, 0, 0x1000, 0, 0],
        // All ones.
        [1, 1, 1, 1, 0x0F00, 0x1000, 1, 1],
        // All 0xFFFFFFFF.
        [u32::MAX, u32::MAX, u32::MAX, u32::MAX, 0x0F00, 0x1000, u32::MAX, u32::MAX],
        // MIN_INT.
        [0x80000000, 0x80000000, 0, 0, 0x0F00, 0x1000, 0, 0],
        // MAX_INT.
        [0x7FFFFFFF, 0x7FFFFFFF, 0, 0, 0x0F00, 0x1000, 0, 0],
        // 1 and -1.
        [1, u32::MAX, 0, 0, 0x0F00, 0x1000, 0, 0],
        // MIN_INT and 1.
        [0x80000000, 1, 0, 0, 0x0F00, 0x1000, 0, 0],
        // Byte boundaries.
        [0xFF, 0xFF00, 0xFF0000, 0xFF000000, 0x0F00, 0x1000, 0, 0],
        // Powers of 2.
        [2, 4, 8, 16, 0x0F00, 0x1000, 32, 64],
        // Alternating bits.
        [0xAAAAAAAA, 0x55555555, 0, 0, 0x0F00, 0x1000, 0, 0],
        // Mixed magic values.
        [0xDEADBEEF, 0xCAFEBABE, 0x12345678, 0x9ABCDEF0, 0x0F00, 0x1000, 0, 0],
        // Small values near zero.
        [0, 1, 2, 3, 0x0F00, 0x1000, 4, 5],
    ]
}

/// Generate N random test vectors.
pub fn random_vectors(n: usize, seed: u64) -> Vec<[u32; 8]> {
    let mut rng = Rng::new(seed);
    let mut vecs = Vec::with_capacity(n);
    for _ in 0..n {
        let mut regs = [0u32; 8];
        for r in &mut regs {
            *r = rng.next_u32();
        }
        // Fix EBP and ESP to known values.
        regs[5] = 0x1000; // ebp
        regs[4] = 0x0F00; // esp
        vecs.push(regs);
    }
    vecs
}

/// Test whether two programs are equivalent on the given test vectors.
/// Both programs are executed from the same initial state, and outputs
/// are compared according to the output mask.
pub fn test_equivalence(
    original: &Program,
    candidate: &Program,
    mask: &OutputMask,
    test_vectors: &[[u32; 8]],
) -> bool {
    for regs in test_vectors {
        let mut state_a = CpuState::new(*regs);
        let mut state_b = CpuState::new(*regs);

        // Seed stack deterministically from register values.
        seed_stack(&mut state_a, regs);
        seed_stack(&mut state_b, regs);

        let result_a = execute(&mut state_a, original);
        let result_b = execute(&mut state_b, candidate);

        match (result_a, result_b) {
            (Ok(()), Ok(())) => {
                if !state_a.equivalent(&state_b, mask) {
                    return false;
                }
            }
            (Err(_), Err(_)) => {
                // Both UB — acceptable.
                continue;
            }
            _ => {
                // One succeeded, one failed — not equivalent.
                return false;
            }
        }
    }
    true
}

/// Quick equivalence check using edge cases + small random set.
pub fn quick_test(
    original: &Program,
    candidate: &Program,
    mask: &OutputMask,
) -> bool {
    let edges = edge_case_vectors();
    if !test_equivalence(original, candidate, mask, &edges) {
        return false;
    }
    let randoms = random_vectors(QUICK_RANDOM_VECTORS, 0xDEAD_BEEF);
    test_equivalence(original, candidate, mask, &randoms)
}

/// Full equivalence check using edge cases + many random vectors.
/// Optionally takes immediate values from the pattern to generate targeted vectors.
pub fn full_test(
    original: &Program,
    candidate: &Program,
    mask: &OutputMask,
) -> bool {
    let edges = edge_case_vectors();
    if !test_equivalence(original, candidate, mask, &edges) {
        return false;
    }
    // Generate targeted vectors using immediates from the original program.
    let imm_vecs = immediate_targeted_vectors(original);
    if !imm_vecs.is_empty() && !test_equivalence(original, candidate, mask, &imm_vecs) {
        return false;
    }
    let randoms = random_vectors(DEFAULT_RANDOM_VECTORS, 0xCAFE_BABE);
    test_equivalence(original, candidate, mask, &randoms)
}

/// Generate test vectors that place the program's immediate values into registers.
/// This catches false equivalences where a specific constant (like cmpl $512) is
/// only distinguishable when registers hold that exact value.
fn immediate_targeted_vectors(prog: &Program) -> Vec<[u32; 8]> {
    use super::ir::SOperand;
    let mut imm_vals: Vec<u32> = Vec::new();
    for instr in &prog.instrs {
        for op in &instr.operands {
            if let SOperand::Imm(v) = op {
                let u = *v as u32;
                imm_vals.push(u);
                imm_vals.push(u.wrapping_add(1));
                imm_vals.push(u.wrapping_sub(1));
            }
        }
    }
    if imm_vals.is_empty() {
        return Vec::new();
    }
    imm_vals.sort();
    imm_vals.dedup();

    let mut vecs = Vec::new();
    for &val in &imm_vals {
        // Place the value in every non-fixed register position.
        let mut regs = [val; 8];
        regs[4] = 0x0F00; // esp
        regs[5] = 0x1000; // ebp
        vecs.push(regs);
        // Also try with only eax set, others zero.
        let mut regs2 = [0u32; 8];
        regs2[0] = val;
        regs2[4] = 0x0F00;
        regs2[5] = 0x1000;
        vecs.push(regs2);
    }
    vecs
}

/// Seed the stack memory deterministically from register values.
/// Also places the register values at common stack offsets so patterns
/// that load from stack then compare will hit the targeted values.
fn seed_stack(state: &mut CpuState, regs: &[u32; 8]) {
    let mut rng = Rng::new(regs[0] as u64 ^ ((regs[1] as u64) << 32));
    for byte in state.stack.iter_mut() {
        *byte = rng.next_u32() as u8;
    }
    // Write DISTINCT values at common stack offsets so identity detection
    // doesn't produce false positives from all slots holding the same value.
    // Uses a hash-like mixing of the offset to produce unique values per slot.
    let common_offsets = [-16, -12, -8, -4, 0, 4, 8, 12, 16, 20, 24, 28, 32, 48, 56, 72, 84, 128];
    for &off in &common_offsets {
        let val = regs[0]
            .wrapping_add(off as u32)
            .wrapping_mul(0x9E3779B9); // golden ratio hash
        let _ = state.write_stack32(off, val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::ir::*;

    #[test]
    fn test_identical_programs_are_equivalent() {
        let prog = Program::new(vec![
            SInstr::new2(Mnemonic::Addl, SOperand::Imm(1), SOperand::Reg(REG_EAX)),
        ]);
        let mask = OutputMask::all();
        assert!(full_test(&prog, &prog, &mask));
    }

    #[test]
    fn test_xorl_self_equivalent_to_movl_zero() {
        // movl $0, %eax  ≡  xorl %eax, %eax  (when only eax and flags matter)
        let mov0 = Program::new(vec![
            SInstr::new2(Mnemonic::Movl, SOperand::Imm(0), SOperand::Reg(REG_EAX)),
        ]);
        let xor0 = Program::new(vec![
            SInstr::new2(Mnemonic::Xorl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_EAX)),
        ]);
        // NOT equivalent on ALL state (movl doesn't set flags, xorl does).
        let mask_all = OutputMask::all();
        assert!(!full_test(&mov0, &xor0, &mask_all));
        // Equivalent if we only check eax and ignore flags.
        let mask_eax = OutputMask::regs_and_flags(&[REG_EAX], false);
        assert!(full_test(&mov0, &xor0, &mask_eax));
    }

    #[test]
    fn test_addl_1_equivalent_to_incl() {
        // addl $1, %eax  ≡  incl %eax  (when CF doesn't matter)
        let add1 = Program::new(vec![
            SInstr::new2(Mnemonic::Addl, SOperand::Imm(1), SOperand::Reg(REG_EAX)),
        ]);
        let inc = Program::new(vec![
            SInstr::new1(Mnemonic::Incl, SOperand::Reg(REG_EAX)),
        ]);
        // NOT equivalent on all flags (addl modifies CF, incl doesn't).
        let mask_all = OutputMask::all();
        assert!(!full_test(&add1, &inc, &mask_all));
        // Equivalent on eax + non-CF flags (ZF, SF, OF, PF).
        // For the superoptimizer, we'd need to know if CF is live.
        let mask_eax = OutputMask::regs_and_flags(&[REG_EAX], false);
        assert!(full_test(&add1, &inc, &mask_eax));
    }

    #[test]
    fn test_different_programs_not_equivalent() {
        let add1 = Program::new(vec![
            SInstr::new2(Mnemonic::Addl, SOperand::Imm(1), SOperand::Reg(REG_EAX)),
        ]);
        let add2 = Program::new(vec![
            SInstr::new2(Mnemonic::Addl, SOperand::Imm(2), SOperand::Reg(REG_EAX)),
        ]);
        let mask = OutputMask::regs_and_flags(&[REG_EAX], false);
        assert!(!full_test(&add1, &add2, &mask));
    }
}
