//! Copy coalescing and immediately-consumed value analysis.
//!
//! Copy coalescing identifies Copy instructions where the destination can
//! share the source's stack slot (eliminating a separate allocation).
//! The immediately-consumed analysis identifies values that are produced
//! and consumed in adjacent instructions, allowing them to skip stack
//! slot allocation entirely by staying in the accumulator register cache.

use crate::ir::reexports::{
    Instruction,
    IrFunction,
    Operand,
    Terminator,
};
use crate::common::types::IrType;
use crate::common::fx_hash::{FxHashMap, FxHashSet};
use crate::backend::regalloc::PhysReg;
use crate::backend::liveness::{
    LivenessResult,
    for_each_operand_in_instruction, for_each_value_use_in_instruction,
    for_each_operand_in_terminator,
};

/// Build the copy alias map: dest_id -> root_id for Copy instructions where
/// dest and src can share the same stack slot.
///
/// Safety: only coalesces when the Copy is the SOLE use of the source value,
/// guaranteeing the source is dead after the Copy (avoids the "lost copy"
/// problem in phi parallel copy groups).
pub(super) fn build_copy_alias_map(
    func: &IrFunction,
    def_block: &FxHashMap<u32, usize>,
    multi_def_values: &FxHashSet<u32>,
    reg_assigned: &FxHashMap<u32, PhysReg>,
    use_blocks_map: &FxHashMap<u32, Vec<usize>>,
    liveness: Option<&LivenessResult>,
) -> FxHashMap<u32, u32> {
    // Count uses of each value across all instructions.
    let mut use_count: FxHashMap<u32, u32> = FxHashMap::default();
    for block in &func.blocks {
        for inst in &block.instructions {
            for_each_operand_in_instruction(inst, |op| {
                if let Operand::Value(v) = op {
                    *use_count.entry(v.0).or_insert(0) += 1;
                }
            });
            for_each_value_use_in_instruction(inst, |v| {
                *use_count.entry(v.0).or_insert(0) += 1;
            });
        }
        for_each_operand_in_terminator(&block.terminator, |op| {
            if let Operand::Value(v) = op {
                *use_count.entry(v.0).or_insert(0) += 1;
            }
        });
    }

    // Build interval-end map for liveness-based coalescing.
    // When liveness is available, we can coalesce a Copy(dest, src) whenever
    // src's live interval ends at the Copy's program point (src dies at the Copy),
    // regardless of how many other uses src had before.
    let interval_ends: Option<FxHashMap<u32, u32>> = liveness.map(|liv| {
        liv.intervals.iter().map(|iv| (iv.value_id, iv.end)).collect()
    });

    // Collect Copy instructions eligible for aliasing.
    // Track program points (same sequencing as liveness analysis) to compare
    // against interval endpoints when liveness is available.
    let mut raw_aliases: Vec<(u32, u32)> = Vec::new();
    let mut point: u32 = 0;
    for block in &func.blocks {
        for inst in &block.instructions {
            let current_point = point;
            point += 1;
            let (d, s) = match inst {
                Instruction::Copy { dest, src: Operand::Value(src_val) } => (dest.0, src_val.0),
                _ => continue,
            };
            // Exclude multi-def values and register-assigned values.
            if multi_def_values.contains(&d) || multi_def_values.contains(&s) {
                continue;
            }
            if reg_assigned.contains_key(&d) || reg_assigned.contains_key(&s) {
                continue;
            }
            // Liveness-based check: src's interval ends at this Copy (src dies here).
            // This is strictly more permissive than the sole-use check: a value can
            // have multiple uses but still die at the Copy if the Copy is its last use.
            let src_dies_at_copy = interval_ends.as_ref().and_then(|ends| {
                Some(*ends.get(&s)? == current_point)
            }).unwrap_or(false);
            // Allow coalescing if src dies at copy (liveness) OR sole use (conservative).
            if !src_dies_at_copy && use_count.get(&s).copied().unwrap_or(0) != 1 {
                continue;
            }
            // Only coalesce if dest's uses are in the same block as source's definition.
            // Cross-block aliasing is unsafe: the root's liveness interval doesn't
            // account for the alias's uses in other blocks.
            if let Some(&src_def_blk) = def_block.get(&s) {
                if let Some(dest_use_blocks) = use_blocks_map.get(&d) {
                    if dest_use_blocks.iter().any(|&b| b != src_def_blk) {
                        continue;
                    }
                }
            }
            raw_aliases.push((d, s));
        }
        point += 1; // terminator
    }

    // Build alias map with transitive resolution: follow chains to find root.
    // Safety limit on chain depth guards against pathological cycles.
    const MAX_ALIAS_CHAIN_DEPTH: usize = 100;
    let mut copy_alias: FxHashMap<u32, u32> = FxHashMap::default();
    for (dest_id, src_id) in raw_aliases {
        let mut root = src_id;
        let mut depth = 0;
        while let Some(&parent) = copy_alias.get(&root) {
            root = parent;
            depth += 1;
            if depth > MAX_ALIAS_CHAIN_DEPTH { break; }
        }
        if root != dest_id {
            copy_alias.insert(dest_id, root);
        }
    }

    // Remove aliases where root or dest is an alloca (alloca slots are special).
    let alloca_ids: FxHashSet<u32> = func.blocks.iter()
        .flat_map(|b| b.instructions.iter())
        .filter_map(|inst| {
            if let Instruction::Alloca { dest, .. } = inst { Some(dest.0) } else { None }
        })
        .collect();
    copy_alias.retain(|dest_id, root_id| {
        !alloca_ids.contains(root_id) && !alloca_ids.contains(dest_id)
    });

    // Remove aliases for InlineAsm output pointer values. InlineAsm Phase 4 reads
    // output pointers from stack slots AFTER the asm executes; if aliased, the
    // root's slot may be reused between the Copy and the InlineAsm, corrupting
    // the pointer read in Phase 4.
    let mut asm_output_ptrs: FxHashSet<u32> = FxHashSet::default();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::InlineAsm { outputs, .. } = inst {
                for (_, v, _) in outputs {
                    asm_output_ptrs.insert(v.0);
                }
            }
        }
    }
    if !asm_output_ptrs.is_empty() {
        copy_alias.retain(|dest_id, _| !asm_output_ptrs.contains(dest_id));
    }

    // Phase 2: Coalesce phi relay values — values whose ONLY definitions are
    // Copy instructions and whose ONLY uses are in Copy instructions.
    // These are pure intermediates created by phi elimination that can share
    // their copy target's slot, eliminating entire copy chains.
    coalesce_phi_relays(func, &mut copy_alias, reg_assigned);

    copy_alias
}

/// Coalesce "phi relay" values with their copy targets.
///
/// A phi relay value is defined ONLY by Copy instructions (never by computation)
/// and used ONLY as the source operand of Copy instructions. These values are
/// pure intermediates created by phi elimination when nested phi nodes generate
/// cascading copies (e.g., `Copy count_next = count_a; Copy count = count_next`).
///
/// By coalescing a relay value V with its copy target W (making them share a
/// slot), all copies in the chain become same-slot no-ops:
/// - `Copy V = X`: stores X into the shared slot (overwrites W, which is dead)
/// - `Copy W = V`: reads and writes the same slot → eliminated
/// - `Copy V = W` (identity path): reads and writes the same slot → eliminated
///
/// Safety: phi elimination places copies at the end of blocks, after all
/// computation. So when `Copy V = X` overwrites the shared slot, W's value
/// has already been read by all uses in that block.
fn coalesce_phi_relays(
    func: &IrFunction,
    copy_alias: &mut FxHashMap<u32, u32>,
    reg_assigned: &FxHashMap<u32, PhysReg>,
) {
    // Step 1: Find which values are defined ONLY by Copy instructions.
    // A value defined by any non-Copy instruction is not a relay.
    let mut only_copy_defs: FxHashSet<u32> = FxHashSet::default();
    let mut has_non_copy_def: FxHashSet<u32> = FxHashSet::default();

    for block in &func.blocks {
        for inst in &block.instructions {
            if let Some(dest) = inst.dest() {
                if matches!(inst, Instruction::Copy { .. }) {
                    if !has_non_copy_def.contains(&dest.0) {
                        only_copy_defs.insert(dest.0);
                    }
                } else {
                    only_copy_defs.remove(&dest.0);
                    has_non_copy_def.insert(dest.0);
                }
            }
        }
    }

    if only_copy_defs.is_empty() {
        return;
    }

    // Step 2: Check which of these "copy-only" values are used ONLY as operands
    // of Copy instructions (not as pointers, not in computation, not in terminators).
    // Collect: for each relay candidate, the set of Copy targets it feeds into.
    let mut relay_targets: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    let mut has_non_copy_use: FxHashSet<u32> = FxHashSet::default();

    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                Instruction::Copy { dest, src: Operand::Value(v) } => {
                    if only_copy_defs.contains(&v.0) && !has_non_copy_use.contains(&v.0) {
                        relay_targets.entry(v.0).or_default().push(dest.0);
                    }
                }
                _ => {
                    // Check if any relay candidate is used as an operand in a non-Copy instruction
                    for_each_operand_in_instruction(inst, |op| {
                        if let Operand::Value(v) = op {
                            if only_copy_defs.contains(&v.0) {
                                has_non_copy_use.insert(v.0);
                                relay_targets.remove(&v.0);
                            }
                        }
                    });
                    // Also check Value-ref uses (ptr in Store/Load, etc.)
                    for_each_value_use_in_instruction(inst, |v| {
                        if only_copy_defs.contains(&v.0) {
                            has_non_copy_use.insert(v.0);
                            relay_targets.remove(&v.0);
                        }
                    });
                }
            }
        }
        // Check terminator operands
        for_each_operand_in_terminator(&block.terminator, |op| {
            if let Operand::Value(v) = op {
                if only_copy_defs.contains(&v.0) {
                    has_non_copy_use.insert(v.0);
                    relay_targets.remove(&v.0);
                }
            }
        });
    }

    // Collect alloca IDs to exclude from coalescing targets.
    let alloca_ids: FxHashSet<u32> = func.blocks.iter()
        .flat_map(|b| b.instructions.iter())
        .filter_map(|inst| {
            if let Instruction::Alloca { dest, .. } = inst { Some(dest.0) } else { None }
        })
        .collect();

    // Step 3: For each relay value with exactly one copy target, coalesce.
    // With multiple targets, pick the one that isn't already aliased.
    const MAX_CHAIN: usize = 100;

    for (relay_id, targets) in &relay_targets {
        if reg_assigned.contains_key(relay_id) {
            continue;
        }

        // Find the best target to coalesce with (prefer non-aliased, non-alloca).
        let mut best_target: Option<u32> = None;
        for &t in targets {
            if alloca_ids.contains(&t) || reg_assigned.contains_key(&t) {
                continue;
            }
            // Resolve the target through existing aliases to find its root.
            let mut root = t;
            let mut depth = 0;
            while let Some(&parent) = copy_alias.get(&root) {
                root = parent;
                depth += 1;
                if depth > MAX_CHAIN { break; }
            }
            // Don't create a self-cycle.
            if root == *relay_id {
                continue;
            }
            if alloca_ids.contains(&root) {
                continue;
            }
            best_target = Some(root);
            break;
        }

        if let Some(target_root) = best_target {
            copy_alias.insert(*relay_id, target_root);
        }
    }
}

/// Identify values that can skip stack slot allocation because they are
/// produced and consumed in adjacent instructions within the same block.
///
/// A value V defined at instruction I can skip its slot if:
/// 1. V has exactly one use as an Operand (loaded via operand_to_rax/rcx)
/// 2. That use is at instruction I+1 (or in the block terminator if I is last)
/// 3. V is the FIRST Operand of the consumer (loaded first into the accumulator)
/// 4. V is NOT used as a Value reference (ptr in Store/Load, base in GEP, etc.)
/// 5. V is not i128/f128 (these need 16-byte slots with special handling)
/// 6. V is not from a Copy instruction (copy aliasing needs the root's slot)
/// 7. V is not from an Alloca (allocas always need addressable slots)
///
/// The codegen accumulator cache ensures correctness: store_rax_to sets the
/// cache, and the next instruction's operand_to_rax finds V there.
pub(super) fn compute_immediately_consumed(func: &IrFunction, lhs_first_binop: bool) -> FxHashSet<u32> {
    let mut result = FxHashSet::default();

    // First pass: count uses per value (both Operand and Value-ref uses).
    let mut operand_use_count: FxHashMap<u32, u32> = FxHashMap::default();
    let mut has_value_ref_use: FxHashSet<u32> = FxHashSet::default();

    for block in &func.blocks {
        for inst in &block.instructions {
            for_each_operand_in_instruction(inst, |op| {
                if let Operand::Value(v) = op {
                    *operand_use_count.entry(v.0).or_insert(0) += 1;
                }
            });
            for_each_value_use_in_instruction(inst, |v| {
                has_value_ref_use.insert(v.0);
            });
        }
        for_each_operand_in_terminator(&block.terminator, |op| {
            if let Operand::Value(v) = op {
                *operand_use_count.entry(v.0).or_insert(0) += 1;
            }
        });
    }

    // Collect all copy-alias roots: values that serve as the slot source for copies.
    // These must keep their slots since aliased copies will use them.
    let mut copy_alias_roots: FxHashSet<u32> = FxHashSet::default();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Copy { src: Operand::Value(v), .. } = inst {
                copy_alias_roots.insert(v.0);
            }
        }
    }

    // Second pass: check adjacency and first-operand conditions.
    for block in &func.blocks {
        let insts = &block.instructions;
        for (i, inst) in insts.iter().enumerate() {
            let dest = match inst.dest() {
                Some(d) => d,
                None => continue,
            };

            // Only acc-preserving producers are safe: after these execute,
            // the accumulator cache still holds the result. Cache-invalidating
            // instructions (Call, Atomic*, DynAlloca, etc.) clear the cache
            // after store_rax_to, so the next instruction can't find the value.
            if !is_acc_preserving_producer(inst) { continue; }
            // Skip i128/f128 (special 16-byte handling uses emit_load_acc_pair /
            // emit_store_acc_pair which bypass the normal accumulator cache).
            if involves_i128_or_f128(inst) { continue; }
            // Skip if value has Value-ref uses (ptr/base in Store/Load/GEP).
            if has_value_ref_use.contains(&dest.0) { continue; }
            // Skip if value is a copy-alias root (other values share its slot).
            if copy_alias_roots.contains(&dest.0) { continue; }
            // Must have exactly one Operand use.
            if operand_use_count.get(&dest.0).copied().unwrap_or(0) != 1 { continue; }

            // Check if the single use is in the immediately next instruction
            // or in the block terminator (if this is the last instruction).
            if i + 1 < insts.len() {
                // Use must be in instruction i+1, as the first Operand.
                let next = &insts[i + 1];
                if is_safe_sole_consumer(next, dest.0, lhs_first_binop) {
                    result.insert(dest.0);
                }
            } else {
                // Last instruction: use must be in the terminator, as the sole Operand.
                if is_sole_operand_of_terminator(&block.terminator, dest.0) {
                    result.insert(dest.0);
                }
            }
        }
    }

    result
}

/// Check if an instruction is an "acc-preserving" producer: after execution,
/// the accumulator register cache still holds the result value. Only these
/// instructions can participate in the skip-slot optimization as producers.
///
/// Cache-invalidating instructions (Call, Store, Atomic*, DynAlloca, InlineAsm,
/// etc.) call invalidate_all() after execution, clearing the cache.
fn is_acc_preserving_producer(inst: &Instruction) -> bool {
    matches!(inst,
        Instruction::Load { .. }
        | Instruction::BinOp { .. }
        | Instruction::UnaryOp { .. }
        | Instruction::Cmp { .. }
        | Instruction::Cast { .. }
        | Instruction::GetElementPtr { .. }
        | Instruction::GlobalAddr { .. }
        | Instruction::Select { .. }
        | Instruction::LabelAddr { .. }
    )
}

/// Check if an instruction involves I128/U128/F128 types in any operand position.
/// These use emit_load_acc_pair / emit_store_acc_pair which bypass the normal
/// accumulator cache, so they cannot participate in skip-slot optimization.
fn involves_i128_or_f128(inst: &Instruction) -> bool {
    fn is_wide(ty: IrType) -> bool {
        matches!(ty, IrType::I128 | IrType::U128 | IrType::F128)
    }
    match inst {
        Instruction::Cast { from_ty, to_ty, .. } => is_wide(*from_ty) || is_wide(*to_ty),
        Instruction::UnaryOp { ty, .. } => is_wide(*ty),
        Instruction::BinOp { ty, .. } => is_wide(*ty),
        Instruction::Cmp { ty, .. } => is_wide(*ty),
        Instruction::Load { ty, .. } => is_wide(*ty),
        _ => {
            // For other instructions, just check the result type.
            matches!(inst.result_type(), Some(ty) if is_wide(ty))
        }
    }
}

/// Check if value_id is the sole Operand loaded by the given instruction,
/// with guaranteed loading order (no other operand loaded before it).
///
/// Only single-operand consumers are safe by default: Store (val loaded first),
/// Cast, UnaryOp, Copy. Two-operand instructions (BinOp, Cmp) are excluded on
/// x86/ARM because codegen may load the OTHER operand first (e.g. BinOp's
/// rhs_conflicts path, float Cmp's Lt/Le operand swap). GEP excluded because
/// OverAligned base computation clobbers %rax before offset is loaded.
///
/// When `lhs_first_binop` is true (RISC-V), BinOp and Cmp are also safe when
/// value_id is the lhs operand, because the RISC-V backend unconditionally
/// loads lhs before rhs with no register-direct conflict paths.
fn is_safe_sole_consumer(inst: &Instruction, value_id: u32, lhs_first_binop: bool) -> bool {
    match inst {
        // Store: val is always loaded first via emit_load_operand (operand_to_rax)
        Instruction::Store { val: Operand::Value(v), .. } => v.0 == value_id,
        // Single-operand instructions: loaded via operand_to_rax, no other operand
        Instruction::Cast { src: Operand::Value(v), .. } => v.0 == value_id,
        Instruction::UnaryOp { src: Operand::Value(v), .. } => v.0 == value_id,
        Instruction::Copy { src: Operand::Value(v), .. } => v.0 == value_id,
        // BinOp: safe on architectures that always load lhs first (RISC-V)
        Instruction::BinOp { lhs: Operand::Value(v), .. } if lhs_first_binop => v.0 == value_id,
        // Cmp: safe on architectures that always load lhs first (RISC-V)
        Instruction::Cmp { lhs: Operand::Value(v), .. } if lhs_first_binop => v.0 == value_id,
        // All other instructions: not safe (GEP, Call, Select, etc.)
        _ => false,
    }
}

/// Check if value_id is the sole operand of a block terminator.
fn is_sole_operand_of_terminator(term: &Terminator, value_id: u32) -> bool {
    match term {
        Terminator::Return(Some(Operand::Value(v))) => v.0 == value_id,
        Terminator::CondBranch { cond: Operand::Value(v), .. } => v.0 == value_id,
        Terminator::Switch { val: Operand::Value(v), .. } => v.0 == value_id,
        Terminator::IndirectBranch { target: Operand::Value(v), .. } => v.0 == value_id,
        _ => false,
    }
}
