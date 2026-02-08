//! Brute-force search engine for the superoptimizer.
//!
//! Enumerates candidate replacement sequences and tests them for equivalence
//! with the original. Finds the shortest byte-count equivalent.

use super::ir::*;
use super::cpu::OutputMask;
use super::sizing::{instruction_size, program_size};
use super::verify::{quick_test, full_test};

/// A discovered rewrite rule.
#[derive(Debug, Clone)]
pub struct RewriteRule {
    /// Original instruction sequence.
    pub original: Program,
    /// Optimal replacement (shorter byte count).
    pub replacement: Program,
    /// Original byte size.
    pub original_size: usize,
    /// Replacement byte size.
    pub replacement_size: usize,
    /// Which outputs must be preserved.
    pub output_mask: OutputMask,
}

impl RewriteRule {
    pub fn savings(&self) -> usize {
        self.original_size - self.replacement_size
    }
}

/// Search for the shortest replacement for a given program.
/// Returns None if no shorter equivalent is found.
pub fn find_optimal(
    original: &Program,
    mask: &OutputMask,
    registers: &[u8],
    immediates: &[i32],
    stack_offsets: &[i32],
) -> Option<RewriteRule> {
    let orig_size = program_size(original)?;
    let mut best: Option<(Program, usize)> = None;
    let best_size = |b: &Option<(Program, usize)>| b.as_ref().map_or(orig_size, |x| x.1);

    // Try empty program (all instructions are dead code).
    let empty = Program::new(vec![]);
    if quick_test(original, &empty, mask) && full_test(original, &empty, mask) {
        return Some(RewriteRule {
            original: original.clone(),
            replacement: empty,
            original_size: orig_size,
            replacement_size: 0,
            output_mask: mask.clone(),
        });
    }

    // Build operand pool.
    let operands = build_operand_pool(registers, immediates, stack_offsets);

    // Try 1-instruction replacements.
    for candidate in enumerate_single(&operands) {
        let cand_size = match instruction_size(&candidate) {
            Some(s) if s < best_size(&best) => s,
            _ => continue,
        };
        let prog = Program::new(vec![candidate]);
        if !quick_test(original, &prog, mask) {
            continue;
        }
        if full_test(original, &prog, mask) {
            best = Some((prog, cand_size));
        }
    }

    // Try 2-instruction replacements (only if original is 3+ instructions
    // or if we haven't found a 1-instruction replacement).
    if original.len() >= 2 {
        for c1 in enumerate_single(&operands) {
            let s1 = match instruction_size(&c1) {
                Some(s) => s,
                None => continue,
            };
            if s1 >= best_size(&best) {
                continue;
            }
            for c2 in enumerate_single(&operands) {
                let s2 = match instruction_size(&c2) {
                    Some(s) => s,
                    None => continue,
                };
                let total = s1 + s2;
                if total >= best_size(&best) {
                    continue;
                }
                let prog = Program::new(vec![c1.clone(), c2]);
                if !quick_test(original, &prog, mask) {
                    continue;
                }
                if full_test(original, &prog, mask) {
                    best = Some((prog, total));
                }
            }
        }
    }

    best.map(|(replacement, replacement_size)| RewriteRule {
        original: original.clone(),
        replacement,
        original_size: orig_size,
        replacement_size,
        output_mask: mask.clone(),
    })
}

/// Build the pool of operands to use in enumeration.
fn build_operand_pool(registers: &[u8], immediates: &[i32], stack_offsets: &[i32]) -> Vec<SOperand> {
    let mut ops = Vec::new();
    for &r in registers {
        ops.push(SOperand::Reg(r));
    }
    for &imm in immediates {
        ops.push(SOperand::Imm(imm));
    }
    for &off in stack_offsets {
        ops.push(SOperand::MemEbp(off));
    }
    ops
}

/// Enumerate all valid single instructions from the operand pool.
fn enumerate_single(operands: &[SOperand]) -> Vec<SInstr> {
    let mut instrs = Vec::new();

    for &mnemonic in Mnemonic::enumerable() {
        match mnemonic.num_operands() {
            0 => {
                instrs.push(SInstr::new0(mnemonic));
            }
            1 => {
                for op in operands {
                    if is_valid_unary(mnemonic, op) {
                        instrs.push(SInstr::new1(mnemonic, *op));
                    }
                }
            }
            2 => {
                for (i, op0) in operands.iter().enumerate() {
                    for (j, op1) in operands.iter().enumerate() {
                        // Skip mem-to-mem (not valid x86).
                        if op0.is_mem() && op1.is_mem() {
                            continue;
                        }
                        // Skip self-moves.
                        if mnemonic == Mnemonic::Movl && op0 == op1 {
                            continue;
                        }
                        // For commutative ops, only try one ordering.
                        if mnemonic.is_commutative() && j < i {
                            continue;
                        }
                        if is_valid_binary(mnemonic, op0, op1) {
                            instrs.push(SInstr::new2(mnemonic, *op0, *op1));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    instrs
}

/// Check if a unary instruction is valid with the given operand.
fn is_valid_unary(mnemonic: Mnemonic, op: &SOperand) -> bool {
    match mnemonic {
        Mnemonic::Incl | Mnemonic::Decl | Mnemonic::Negl | Mnemonic::Notl => {
            matches!(op, SOperand::Reg(_) | SOperand::MemEbp(_))
        }
        Mnemonic::Pushl => matches!(op, SOperand::Reg(_) | SOperand::Imm(_)),
        Mnemonic::Popl => matches!(op, SOperand::Reg(_)),
        _ => false,
    }
}

/// Check if a binary instruction is valid with the given operands.
fn is_valid_binary(mnemonic: Mnemonic, src: &SOperand, dst: &SOperand) -> bool {
    match mnemonic {
        Mnemonic::Movl => {
            // src: reg/imm/mem, dst: reg/mem (not mem-to-mem)
            match (src, dst) {
                (SOperand::Reg(_), SOperand::Reg(_)) => true,
                (SOperand::Imm(_), SOperand::Reg(_)) => true,
                (SOperand::Reg(_), SOperand::MemEbp(_)) => true,
                (SOperand::MemEbp(_), SOperand::Reg(_)) => true,
                (SOperand::Imm(_), SOperand::MemEbp(_)) => true,
                _ => false,
            }
        }
        Mnemonic::Addl | Mnemonic::Subl | Mnemonic::Xorl
        | Mnemonic::Andl | Mnemonic::Orl | Mnemonic::Cmpl => {
            match (src, dst) {
                (SOperand::Reg(_), SOperand::Reg(_)) => true,
                (SOperand::Imm(_), SOperand::Reg(_)) => true,
                (SOperand::Reg(_), SOperand::MemEbp(_)) => true,
                (SOperand::MemEbp(_), SOperand::Reg(_)) => true,
                (SOperand::Imm(_), SOperand::MemEbp(_)) => true,
                _ => false,
            }
        }
        Mnemonic::Testl => {
            matches!((src, dst),
                (SOperand::Reg(_), SOperand::Reg(_))
                | (SOperand::Imm(_), SOperand::Reg(_)))
        }
        Mnemonic::Shll | Mnemonic::Shrl | Mnemonic::Sarl => {
            // src: imm or %cl, dst: reg
            match (src, dst) {
                (SOperand::Imm(_), SOperand::Reg(_)) => true,
                (SOperand::Reg(REG_ECX), SOperand::Reg(_)) => true,
                _ => false,
            }
        }
        Mnemonic::Leal => {
            matches!(dst, SOperand::Reg(_)) && matches!(src, SOperand::MemEbp(_) | SOperand::LeaMem(..))
        }
        Mnemonic::Imull => {
            matches!((src, dst), (SOperand::Reg(_), SOperand::Reg(_)))
        }
        Mnemonic::Movzbl | Mnemonic::Movsbl => {
            matches!(dst, SOperand::Reg(_)) && matches!(src, SOperand::Reg(_) | SOperand::MemEbp(_))
        }
        Mnemonic::Xchgl => {
            matches!((src, dst), (SOperand::Reg(_), SOperand::Reg(_)))
        }
        _ => false,
    }
}

/// Run the search on a set of input patterns.
/// Returns all discovered rewrite rules.
pub fn search_patterns(patterns: &[(Program, OutputMask, Vec<u8>, Vec<i32>, Vec<i32>)]) -> Vec<RewriteRule> {
    let mut rules = Vec::new();
    for (i, (prog, mask, regs, imms, offsets)) in patterns.iter().enumerate() {
        if let Some(rule) = find_optimal(prog, mask, regs, imms, offsets) {
            eprintln!(
                "[{}/{}] Found: {} bytes -> {} bytes (saves {})",
                i + 1,
                patterns.len(),
                rule.original_size,
                rule.replacement_size,
                rule.savings(),
            );
            eprintln!("  Original:    {}", rule.original);
            eprintln!("  Replacement: {}", rule.replacement);
            rules.push(rule);
        }
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rediscover_xorl_for_zero() {
        // movl $0, %eax → should find xorl %eax, %eax
        let original = Program::new(vec![
            SInstr::new2(Mnemonic::Movl, SOperand::Imm(0), SOperand::Reg(REG_EAX)),
        ]);
        let mask = OutputMask::regs_and_flags(&[REG_EAX], false);
        let rule = find_optimal(
            &original,
            &mask,
            &[REG_EAX],
            &[0, 1, -1],
            &[],
        );
        assert!(rule.is_some(), "should find a shorter replacement");
        let rule = rule.unwrap();
        assert!(rule.savings() > 0, "should save bytes");
        eprintln!("Found: {} -> {} (saves {})", rule.original, rule.replacement, rule.savings());
    }

    #[test]
    fn test_rediscover_incl() {
        // addl $1, %eax → should find incl %eax
        let original = Program::new(vec![
            SInstr::new2(Mnemonic::Addl, SOperand::Imm(1), SOperand::Reg(REG_EAX)),
        ]);
        let mask = OutputMask::regs_and_flags(&[REG_EAX], false);
        let rule = find_optimal(
            &original,
            &mask,
            &[REG_EAX],
            &[0, 1, -1],
            &[],
        );
        assert!(rule.is_some(), "should find incl");
        let rule = rule.unwrap();
        assert_eq!(rule.replacement_size, 1, "incl %eax is 1 byte");
        assert_eq!(rule.savings(), 2, "saves 2 bytes (3 -> 1)");
    }

    #[test]
    fn test_rediscover_decl() {
        // subl $1, %eax → should find decl %eax
        let original = Program::new(vec![
            SInstr::new2(Mnemonic::Subl, SOperand::Imm(1), SOperand::Reg(REG_EAX)),
        ]);
        let mask = OutputMask::regs_and_flags(&[REG_EAX], false);
        let rule = find_optimal(
            &original,
            &mask,
            &[REG_EAX],
            &[0, 1, -1],
            &[],
        );
        assert!(rule.is_some(), "should find decl");
        let rule = rule.unwrap();
        assert_eq!(rule.replacement_size, 1);
    }

    #[test]
    fn test_dead_code_elimination() {
        // movl %eax, %ecx; movl %edx, %ecx → second instruction only
        let original = Program::new(vec![
            SInstr::new2(Mnemonic::Movl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_ECX)),
            SInstr::new2(Mnemonic::Movl, SOperand::Reg(REG_EDX), SOperand::Reg(REG_ECX)),
        ]);
        let mask = OutputMask::regs_and_flags(&[REG_ECX], false);
        let rule = find_optimal(
            &original,
            &mask,
            &[REG_EAX, REG_ECX, REG_EDX],
            &[],
            &[],
        );
        assert!(rule.is_some(), "should find shorter");
        let rule = rule.unwrap();
        assert_eq!(rule.replacement.len(), 1, "only need the second mov");
    }
}
