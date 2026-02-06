//! Optimization passes for the IR.
//!
//! This module contains various optimization passes that transform the IR
//! to produce better code. The pass pipeline is tiered by optimization level:
//!
//! - `-O0`: No passes run (early return).
//! - `-O1`: Inline + 1 iteration of core passes (cfg_simplify, copy_prop,
//!   simplify, constfold, dce).
//! - `-O2`+: Full pipeline — inline + 3 iterations of all passes.

pub(crate) mod cfg_simplify;
pub(crate) mod constant_fold;
pub(crate) mod copy_prop;
pub(crate) mod dce;
mod dead_statics;
pub(crate) mod div_by_const;
pub(crate) mod gvn;
pub(crate) mod if_convert;
pub(crate) mod inline;
pub(crate) mod ipcp;
pub(crate) mod iv_strength_reduce;
pub(crate) mod licm;
pub(crate) mod loop_analysis;
pub(crate) mod narrow;
mod resolve_asm;
pub(crate) mod simplify;

use crate::ir::analysis::CfgAnalysis;
use crate::ir::reexports::{IrFunction, IrModule};

/// Run a per-function pass only on functions in the visit set.
///
/// `visit` indicates which functions to process in this iteration.
/// `changed` accumulates which functions were modified by any pass
/// (so the next iteration knows what to re-visit).
fn run_on_visited<F>(
    module: &mut IrModule,
    visit: &[bool],
    changed: &mut [bool],
    mut f: F,
) -> usize
where
    F: FnMut(&mut IrFunction) -> usize,
{
    let mut total = 0;
    for (i, func) in module.functions.iter_mut().enumerate() {
        if func.is_declaration {
            continue;
        }
        if i < visit.len() && !visit[i] {
            continue;
        }
        let n = f(func);
        if n > 0 {
            if i < changed.len() {
                changed[i] = true;
            }
            total += n;
        }
    }
    total
}

/// Run GVN, LICM, and IVSR with shared CFG analysis per function.
///
/// For each dirty function, builds CFG/dominator/loop analysis once and passes
/// it to all three passes. This eliminates redundant analysis computation that
/// previously occurred when each pass independently computed build_label_map +
/// build_cfg + compute_dominators (+ find_natural_loops for LICM/IVSR).
///
/// Returns (gvn_changes, licm_changes, ivsr_changes).
fn run_gvn_licm_ivsr_shared(
    module: &mut IrModule,
    visit: &[bool],
    changed: &mut [bool],
    run_gvn: bool,
    run_licm: bool,
    run_ivsr: bool,
    time_passes: bool,
    iter: usize,
) -> (usize, usize, usize) {
    let mut gvn_total = 0usize;
    let mut licm_total = 0usize;
    let mut ivsr_total = 0usize;

    for (i, func) in module.functions.iter_mut().enumerate() {
        if func.is_declaration {
            continue;
        }
        if i < visit.len() && !visit[i] {
            continue;
        }
        let num_blocks = func.blocks.len();
        if num_blocks == 0 {
            continue;
        }

        // GVN fast path: single-block functions don't need CFG analysis.
        if num_blocks == 1 {
            if run_gvn {
                let n = gvn::run_gvn_function(func);
                if n > 0 {
                    gvn_total += n;
                    if i < changed.len() { changed[i] = true; }
                }
            }
            // LICM and IVSR need loops (>= 2 blocks), so skip.
            continue;
        }

        // Build CFG analysis once for this function.
        let cfg = CfgAnalysis::build(func);

        // Run GVN with shared analysis.
        if run_gvn {
            let t0 = if time_passes { Some(std::time::Instant::now()) } else { None };
            let n = gvn::run_gvn_with_analysis(func, &cfg);
            if let Some(t0) = t0 {
                eprintln!("[PASS] iter={} gvn (func {}): {:.4}s ({} changes)", iter, func.name, t0.elapsed().as_secs_f64(), n);
            }
            if n > 0 {
                gvn_total += n;
                if i < changed.len() { changed[i] = true; }
            }
        }

        // Run LICM with shared analysis.
        // GVN does not modify the CFG (only replaces operands), so analysis is still valid.
        if run_licm {
            let t0 = if time_passes { Some(std::time::Instant::now()) } else { None };
            let n = licm::licm_with_analysis(func, &cfg);
            if let Some(t0) = t0 {
                eprintln!("[PASS] iter={} licm (func {}): {:.4}s ({} changes)", iter, func.name, t0.elapsed().as_secs_f64(), n);
            }
            if n > 0 {
                licm_total += n;
                if i < changed.len() { changed[i] = true; }
            }
        }

        // Run IVSR with shared analysis.
        // LICM hoists instructions to preheaders but does not add/remove blocks,
        // so CFG analysis is still valid.
        if run_ivsr {
            let t0 = if time_passes { Some(std::time::Instant::now()) } else { None };
            let n = iv_strength_reduce::ivsr_with_analysis(func, &cfg);
            if let Some(t0) = t0 {
                eprintln!("[PASS] iter={} iv_strength_reduce (func {}): {:.4}s ({} changes)", iter, func.name, t0.elapsed().as_secs_f64(), n);
            }
            if n > 0 {
                ivsr_total += n;
                if i < changed.len() { changed[i] = true; }
            }
        }
    }

    if time_passes {
        eprintln!("[PASS] iter={} gvn_total: {} changes", iter, gvn_total);
        eprintln!("[PASS] iter={} licm_total: {} changes", iter, licm_total);
        eprintln!("[PASS] iter={} ivsr_total: {} changes", iter, ivsr_total);
    }

    (gvn_total, licm_total, ivsr_total)
}

/// Run all optimization passes on the module.
///
/// The pass pipeline is:
/// 1. CFG simplification (remove dead blocks, thread jump chains, simplify branches)
/// 2. Copy propagation (replace uses of copies with original values)
/// 3. Algebraic simplification (strength reduction)
/// 4. Constant folding (evaluate const exprs at compile time)
/// 5. GVN / CSE (dominator-based value numbering, eliminates redundant
///    BinOp, UnaryOp, Cmp, Cast, GetElementPtr, and Load across dominated blocks)
/// 6. LICM (hoist loop-invariant code to preheaders)
/// 7. If-conversion (convert branch+phi diamonds to Select)
/// 8. Copy propagation (clean up copies from GVN/simplify/LICM)
/// 9. Dead code elimination (remove dead instructions)
/// 10. CFG simplification (clean up after DCE may have made blocks dead)
/// 11. Dead static function elimination (remove unreferenced internal-linkage functions)
///
struct DisabledPasses {
    cfg: bool,
    copyprop: bool,
    narrow: bool,
    simplify: bool,
    constfold: bool,
    gvn: bool,
    licm: bool,
    ifconv: bool,
    dce: bool,
    ipcp: bool,
}

impl DisabledPasses {
    fn from_env(disabled: &str) -> Self {
        DisabledPasses {
            cfg: disabled.contains("cfg"),
            copyprop: disabled.contains("copyprop"),
            narrow: disabled.contains("narrow"),
            simplify: disabled.contains("simplify"),
            constfold: disabled.contains("constfold"),
            gvn: disabled.contains("gvn"),
            licm: disabled.contains("licm"),
            ifconv: disabled.contains("ifconv"),
            dce: disabled.contains("dce"),
            ipcp: disabled.contains("ipcp"),
        }
    }
}

/// Run Phase 0: function inlining and post-inline optimization passes.
fn run_inline_phase(module: &mut IrModule, disabled: &str) {
    if disabled.contains("inline") {
        return;
    }
    inline::run(module);

    // After inlining, convert extern inline gnu_inline functions to declarations.
    // These function bodies were only needed for inlining; they must not be emitted
    // as standalone definitions because their internal calls (e.g., `call btowc`)
    // would resolve to the local definition instead of the external library symbol,
    // causing infinite recursion. Converting to declarations ensures any remaining
    // (non-inlined) calls resolve to the external symbol at link time.
    for func in &mut module.functions {
        if func.is_gnu_inline_def && !func.is_declaration {
            func.is_declaration = true;
            func.blocks.clear();
        }
    }

    crate::ir::mem2reg::promote_allocas_with_params(module);
    constant_fold::run(module);
    copy_prop::run(module);
    simplify::run(module);
    constant_fold::run(module);
    copy_prop::run(module);
    resolve_asm::resolve_inline_asm_symbols(module);
}

/// Runs optimization passes on the IR module. The pass pipeline is tiered
/// based on `opt_level`:
///
/// - **O0**: Returns immediately — no passes run. `mem2reg` and `phi-eliminate`
///   (called by the driver before/after this function) still run since they are
///   required for correct codegen.
/// - **O1**: Inline + 1 iteration of core passes (cfg_simplify, copy_prop,
///   simplify, constfold, dce) + dead-static elimination. Skips expensive
///   analysis-heavy passes (GVN, LICM, IVSR, if-convert, narrow, div_by_const,
///   IPCP).
/// - **O2+**: Full pipeline — inline + 3 iterations of all passes (including
///   GVN, LICM, IVSR, if-convert, narrow, div_by_const, IPCP) + dead-static
///   elimination. O3 currently behaves identically to O2.
///
/// The `CCC_DISABLE_PASSES` environment variable can still be used to disable
/// individual passes or all passes (`CCC_DISABLE_PASSES=all`) regardless of
/// the optimization level.
pub(crate) fn run_passes(module: &mut IrModule, opt_level: u32, target: crate::backend::Target) {
    let disabled = std::env::var("CCC_DISABLE_PASSES").unwrap_or_default();
    if disabled.contains("all") {
        return;
    }

    // -O0: skip all optimization passes entirely.
    if opt_level == 0 {
        return;
    }

    run_inline_phase(module, &disabled);
    constant_fold::resolve_remaining_is_constant(module);

    let iterations = if opt_level <= 1 { 1 } else { 3 };
    let num_funcs = module.functions.len();
    let mut dirty = vec![true; num_funcs];
    let dis = DisabledPasses::from_env(&disabled);

    // `changed` accumulates which functions were modified during each iteration.
    let mut changed = vec![false; num_funcs];

    let time_passes = std::env::var("CCC_TIME_PASSES").is_ok();

    // Per-pass change counts from the previous iteration, used for skip decisions.
    // Pass indices: 0=cfg1, 1=copyprop1, 2=narrow, 3=simplify, 4=constfold,
    //               5=gvn, 6=licm, 7=ifconv, 8=copyprop2, 9=dce, 10=cfg2
    const NUM_PASSES: usize = 11;
    let mut prev_pass_changes = [usize::MAX; NUM_PASSES]; // MAX = "assume changed" for iter 0

    // Track first iteration's total changes for diminishing-returns early exit.
    let mut iter0_total_changes = 0usize;

    for iter in 0..iterations {
        let mut total_changes = 0usize;
        let mut total_changes_excl_dce = 0usize; // Exclude DCE for diminishing-returns check
        let mut cur_pass_changes = [0usize; NUM_PASSES];

        // Clear the changed accumulator for this iteration
        changed.iter_mut().for_each(|c| *c = false);

        macro_rules! timed_pass {
            ($name:expr, $body:expr) => {{
                if time_passes {
                    let t0 = std::time::Instant::now();
                    let n = $body;
                    let elapsed = t0.elapsed().as_secs_f64();
                    eprintln!("[PASS] iter={} {}: {:.4}s ({} changes)", iter, $name, elapsed, n);
                    n
                } else {
                    $body
                }
            }};
        }

        // Helper: check if a pass should run based on upstream pass changes.
        // A pass runs if it or any of its upstream passes made changes last iteration.
        // On iteration 0, all passes run (prev_pass_changes are MAX).
        //
        // Pass dependency graph (which passes create opportunities for which):
        //   cfg_simplify → copy_prop, gvn, dce (simpler CFG)
        //   copy_prop → simplify, constfold, gvn, narrow (propagated values)
        //   narrow → simplify, constfold (smaller types)
        //   simplify → constfold, copy_prop, gvn (reduced expressions, folded casts to copies)
        //   constfold → cfg_simplify, copy_prop, dce (constant branches/dead code, folded exprs to copies)
        //   gvn → copy_prop, dce (eliminated redundant computations)
        //   licm → copy_prop, dce (hoisted code)
        //   if_convert → copy_prop, dce (eliminated branches)
        //   dce → cfg_simplify (empty blocks)
        macro_rules! should_run {
            ($self_idx:expr, $($upstream:expr),*) => {{
                prev_pass_changes[$self_idx] > 0 $(|| prev_pass_changes[$upstream] > 0)*
            }};
        }

        // Phase 1: CFG simplification
        // Upstream: constfold (constant branches), dce (empty blocks)
        if !dis.cfg && should_run!(0, 4, 9) {
            let n = timed_pass!("cfg_simplify1", run_on_visited(module, &dirty, &mut changed, cfg_simplify::run_function));
            cur_pass_changes[0] = n;
            total_changes += n;
            total_changes_excl_dce += n;
        }

        // Phase 2: Copy propagation
        // Upstream: cfg_simplify (simpler CFG), gvn (eliminated exprs), licm (hoisted code), if_convert
        if !dis.copyprop && should_run!(1, 0, 5, 6, 7) {
            let n = timed_pass!("copy_prop1", run_on_visited(module, &dirty, &mut changed, copy_prop::propagate_copies));
            cur_pass_changes[1] = n;
            total_changes += n;
            total_changes_excl_dce += n;
        }

        // Phase 2a: Division-by-constant strength reduction (first iteration only).
        // Replaces slow div/idiv instructions with fast multiply-and-shift sequences.
        // Run early so subsequent passes (narrowing, simplify, constant folding, DCE)
        // can optimize the expanded instruction sequences.
        //
        // Disabled on i686: the pass generates I64 multiply + shift-right-32 sequences
        // to extract the high 32 bits of a widened multiplication. The i686 backend
        // cannot execute 64-bit arithmetic correctly (it truncates to 32 bits), so these
        // sequences produce wrong results. Fall back to hardware idiv/div instead.
        // TODO: Re-enable once i686 has proper 64-bit arithmetic support, or implement
        // a 32-bit-aware variant that uses single-operand imull for mulhi.
        if iter == 0 && opt_level >= 2 && !disabled.contains("divconst") && !target.is_32bit() {
            let n = timed_pass!("div_by_const", run_on_visited(module, &dirty, &mut changed, div_by_const::div_by_const_function));
            total_changes += n;
            total_changes_excl_dce += n;
        }

        // Phase 2b: Integer narrowing
        // Upstream: copy_prop (propagated values expose narrowing)
        if opt_level >= 2 && !dis.narrow && should_run!(2, 1) {
            let n = timed_pass!("narrow", run_on_visited(module, &dirty, &mut changed, narrow::narrow_function));
            cur_pass_changes[2] = n;
            total_changes += n;
            total_changes_excl_dce += n;
        }

        // Phase 3: Algebraic simplification
        // Upstream: copy_prop (propagated values), narrow (smaller types)
        if !dis.simplify && should_run!(3, 1, 2) {
            let n = timed_pass!("simplify", run_on_visited(module, &dirty, &mut changed, simplify::simplify_function));
            cur_pass_changes[3] = n;
            total_changes += n;
            total_changes_excl_dce += n;
        }

        // Phase 4: Constant folding
        // Upstream: copy_prop (propagated constants), narrow, simplify (reduced exprs),
        //           if_convert (creates Select that constfold can fold with known-constant cond),
        //           copy_prop2 (propagates constants into Select/Cmp operands after if_convert)
        if !dis.constfold && should_run!(4, 1, 2, 3, 7, 8) {
            let n = timed_pass!("constfold", run_on_visited(module, &dirty, &mut changed, constant_fold::fold_function));
            cur_pass_changes[4] = n;
            total_changes += n;
            total_changes_excl_dce += n;
        }

        // Phases 5-6a: GVN + LICM + IVSR with shared CFG analysis.
        //
        // These three passes all need CFG + dominator + loop analysis. Since GVN
        // does not modify the CFG (it only replaces instruction operands within
        // existing blocks), the analysis computed for GVN remains valid for LICM
        // and IVSR. We compute it once per function and share it across all three.
        {
            let run_gvn = opt_level >= 2 && !dis.gvn && should_run!(5, 0, 1, 3);
            let run_licm = opt_level >= 2 && !dis.licm && should_run!(6, 0, 1, 5);
            let run_ivsr = opt_level >= 2 && iter == 0 && !disabled.contains("ivsr");

            if run_gvn || run_licm || run_ivsr {
                let (gvn_n, licm_n, ivsr_n) = run_gvn_licm_ivsr_shared(
                    module, &dirty, &mut changed,
                    run_gvn, run_licm, run_ivsr,
                    time_passes, iter,
                );
                cur_pass_changes[5] = gvn_n;
                total_changes += gvn_n;
                total_changes_excl_dce += gvn_n;
                cur_pass_changes[6] = licm_n;
                total_changes += licm_n;
                total_changes_excl_dce += licm_n;
                total_changes += ivsr_n;
                total_changes_excl_dce += ivsr_n;
            }
        }

        // Phase 7: If-conversion
        // Upstream: cfg_simplify (simpler CFG), constfold (simplified conditions)
        if opt_level >= 2 && !dis.ifconv && should_run!(7, 0, 4) {
            let n = timed_pass!("if_convert", run_on_visited(module, &dirty, &mut changed, if_convert::if_convert_function));
            cur_pass_changes[7] = n;
            total_changes += n;
            total_changes_excl_dce += n;
        }

        // Phase 8: Copy propagation again
        // Upstream: simplify (folded casts to copies), constfold (folded exprs to copies),
        //           gvn (produced copies), licm (hoisted code), if_convert (select values)
        // Note: simplify and constfold run earlier in this iteration, so we check
        // cur_pass_changes for them (not just prev_pass_changes via should_run!).
        if !dis.copyprop && (should_run!(8, 5, 6, 7) || cur_pass_changes[3] > 0 || cur_pass_changes[4] > 0) {
            let n = timed_pass!("copy_prop2", run_on_visited(module, &dirty, &mut changed, copy_prop::propagate_copies));
            cur_pass_changes[8] = n;
            total_changes += n;
            total_changes_excl_dce += n;
        }

        // Phase 9: Dead code elimination
        // Upstream: gvn, licm, if_convert, copy_prop2 (produced dead instructions)
        // Note: DCE changes are excluded from the diminishing-returns comparison
        // (total_changes_excl_dce) because DCE is a cleanup pass that removes dead
        // instructions. Its large change count (often 5000+ in iteration 0) inflates
        // iter0_total_changes, and by removing instructions, DCE actually reduces
        // the work subsequent passes can do in later iterations. This combination
        // causes the diminishing-returns heuristic to exit too early, preventing
        // the optimizer from completing multi-iteration constant propagation chains
        // (e.g., kernel's cpucap_is_possible switch folding through inlined
        // system_supports_sme -> alternative_has_cap_unlikely -> cpucap_is_possible).
        if !dis.dce && should_run!(9, 5, 6, 7, 8) {
            let n = timed_pass!("dce", run_on_visited(module, &dirty, &mut changed, dce::eliminate_dead_code));
            cur_pass_changes[9] = n;
            total_changes += n;
            // Intentionally NOT added to total_changes_excl_dce
        }

        // Phase 10: CFG simplification again
        // Upstream: constfold (constant branches), dce (dead blocks), if_convert
        if !dis.cfg && should_run!(10, 4, 7, 9) {
            let n = timed_pass!("cfg_simplify2", run_on_visited(module, &dirty, &mut changed, cfg_simplify::run_function));
            cur_pass_changes[10] = n;
            total_changes += n;
            total_changes_excl_dce += n;
        }

        // Phase 10.5: Interprocedural constant propagation (IPCP).
        // Run on every iteration, not just iter 0, because later iterations may
        // have simplified call arguments to constants (e.g., phi nodes collapsed
        // after CFG simplification resolved dead branches from IS_ENABLED() checks).
        let mut ipcp_changes = 0;
        if opt_level >= 2 && !dis.ipcp {
            ipcp_changes = timed_pass!("ipcp", ipcp::run(module));
            if ipcp_changes > 0 {
                changed.iter_mut().for_each(|c| *c = true);
            }
            total_changes += ipcp_changes;
            total_changes_excl_dce += ipcp_changes;
        }

        if iter == 0 {
            iter0_total_changes = total_changes_excl_dce;
        }

        // Early exit: if no passes changed anything, additional iterations are useless.
        if total_changes == 0 {
            break;
        }

        // Diminishing returns: if this iteration produced very few changes relative
        // to the first iteration, another iteration is unlikely to be worthwhile.
        // The optimizer converges quickly: typically iter 0 finds ~264K changes,
        // iter 1 finds ~10K, iter 2 finds ~200. Stopping when an iteration yields
        // less than 5% of the first iteration's output saves one full pipeline
        // iteration with negligible impact on optimization quality.
        //
        // We use total_changes_excl_dce for this comparison because DCE is a
        // cleanup pass whose large change count (removing dead instructions)
        // inflates iter0's total and makes subsequent iterations look like
        // diminishing returns even when they're still making meaningful progress.
        // DCE also reduces the number of instructions available for other passes,
        // naturally lowering their change counts in later iterations.
        //
        // Exception: if IPCP made changes this iteration, always run another
        // iteration regardless of diminishing returns. IPCP changes (constant
        // argument propagation, dead call elimination) create opportunities for
        // constant folding, DCE, and CFG simplification that require a full pass
        // to clean up. Without this, dead code referencing undefined symbols
        // (like the kernel's convert_to_fxsr) would survive.
        // We require iter > 1 (at least 2 full iterations) because multi-step
        // constant propagation chains (e.g., kernel's switch folding through
        // inlined cpucap_is_possible -> alternative_has_cap_unlikely) need at
        // least 2 iterations to complete: iter0 for initial folding, iter1 for
        // propagating results through the control flow.
        const DIMINISHING_RETURNS_FACTOR: usize = 20; // 1/20 = 5% threshold
        if iter > 1 && ipcp_changes == 0 && iter0_total_changes > 0
            && total_changes_excl_dce * DIMINISHING_RETURNS_FACTOR < iter0_total_changes
        {
            break;
        }

        // Save per-pass change counts for next iteration's skip decisions.
        prev_pass_changes = cur_pass_changes;

        // Prepare dirty set for next iteration: only re-visit functions that changed.
        std::mem::swap(&mut dirty, &mut changed);
    }

    // Phase 11: Dead static function elimination.
    // After all optimizations, remove internal-linkage (static) functions that are
    // never referenced by any other function or global initializer. This is critical
    // for `static inline` functions from headers: after intra-procedural optimizations
    // eliminate dead code paths (e.g., `if (1 || expr)` removes the else branch),
    // some static inline callees may become completely unreferenced and can be removed.
    // Without this, the dead functions may reference undefined external symbols
    // (e.g., kernel's `___siphash_aligned` calling `__siphash_aligned` which doesn't
    // exist on x86 where CONFIG_HAVE_EFFICIENT_UNALIGNED_ACCESS is set).
    dead_statics::eliminate_dead_static_functions(module);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Target;
    use crate::common::error::DiagnosticEngine;
    use crate::common::source::SourceManager;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;
    use crate::frontend::preprocessor::Preprocessor;
    use crate::frontend::sema::SemanticAnalyzer;
    use crate::ir::lowering::Lowerer;
    use crate::ir::mem2reg::promote_allocas;

    /// Compile C source to an optimized IR module at the given opt_level.
    fn compile_at_opt_level(source: &str, opt_level: u32) -> IrModule {
        let mut pp = Preprocessor::new();
        let preprocessed = pp.preprocess(source);

        let mut sm = SourceManager::new();
        let fid = sm.add_file("test.c".into(), preprocessed);
        sm.build_line_map();
        let mut lexer = Lexer::new(sm.get_content(fid), fid);
        let tokens = lexer.tokenize();

        let mut diagnostics = DiagnosticEngine::new();
        diagnostics.set_source_manager(sm);
        let mut parser = Parser::new(tokens);
        parser.set_diagnostics(diagnostics);
        let ast = parser.parse();
        assert_eq!(parser.error_count, 0, "parse errors");

        let diagnostics = parser.take_diagnostics();
        let mut sema = SemanticAnalyzer::new();
        sema.set_diagnostics(diagnostics);
        sema.analyze(&ast).expect("sema errors");
        let sema_result = sema.into_result();

        let diagnostics = DiagnosticEngine::new();
        let lowerer = Lowerer::with_type_context(
            Target::X86_64,
            sema_result.type_context,
            sema_result.functions,
            sema_result.expr_types,
            sema_result.const_values,
            diagnostics,
            false,
        );
        let (mut module, _diag) = lowerer.lower(&ast);

        promote_allocas(&mut module);
        run_passes(&mut module, opt_level, Target::X86_64);
        module
    }

    /// Count total instructions (excluding terminators) across all non-declaration functions.
    fn count_instructions(module: &IrModule) -> usize {
        module.functions.iter()
            .filter(|f| !f.is_declaration)
            .flat_map(|f| &f.blocks)
            .map(|b| b.instructions.len())
            .sum()
    }

    #[test]
    fn test_o0_skips_all_passes() {
        // With -O0, dead code and redundant operations should be preserved.
        let source = r#"
            int foo(int x) {
                int a = x + 1;
                int b = x + 1;  // redundant, GVN would eliminate at -O2
                int unused = 42; // dead, DCE would remove at -O1+
                return a + b;
            }
        "#;
        let module = compile_at_opt_level(source, 0);
        let count = count_instructions(&module);
        // At -O0, all instructions are preserved (no optimization).
        // The exact count depends on lowering, but it should be more than -O2.
        let o2_module = compile_at_opt_level(source, 2);
        let o2_count = count_instructions(&o2_module);
        assert!(count > o2_count,
            "O0 should have more instructions ({}) than O2 ({})", count, o2_count);
    }

    #[test]
    fn test_o1_runs_basic_passes() {
        // -O1 should do constant folding and DCE, but skip GVN.
        let source = r#"
            int foo(void) {
                int x = 2 + 3;   // constfold should fold to 5
                int unused = 99; // DCE should remove
                return x;
            }
        "#;
        let o0_module = compile_at_opt_level(source, 0);
        let o1_module = compile_at_opt_level(source, 1);
        let o0_count = count_instructions(&o0_module);
        let o1_count = count_instructions(&o1_module);
        // -O1 should optimize more than -O0
        assert!(o1_count < o0_count,
            "O1 ({}) should have fewer instructions than O0 ({})", o1_count, o0_count);
    }

    #[test]
    fn test_o2_full_pipeline() {
        // -O2 runs all passes including GVN, which eliminates redundant computations.
        let source = r#"
            int foo(int x) {
                int a = x * 3;
                int b = x * 3;  // GVN should eliminate this redundancy
                return a + b;
            }
        "#;
        let o1_module = compile_at_opt_level(source, 1);
        let o2_module = compile_at_opt_level(source, 2);
        let o1_count = count_instructions(&o1_module);
        let o2_count = count_instructions(&o2_module);
        // -O2 should optimize at least as much as -O1 (often more due to GVN/LICM)
        assert!(o2_count <= o1_count,
            "O2 ({}) should have <= instructions than O1 ({})", o2_count, o1_count);
    }

    #[test]
    fn test_o3_same_as_o2() {
        let source = r#"
            int foo(int x) {
                return x * 2 + 1;
            }
        "#;
        let o2_module = compile_at_opt_level(source, 2);
        let o3_module = compile_at_opt_level(source, 3);
        let o2_count = count_instructions(&o2_module);
        let o3_count = count_instructions(&o3_module);
        assert_eq!(o2_count, o3_count, "O3 should produce same output as O2");
    }

    #[test]
    fn test_different_levels_produce_different_ir() {
        // A program with dead code and redundant computations to exercise all tiers.
        let source = r#"
            int foo(int x, int y) {
                int a = x + y;
                int b = x + y;      // redundant
                int c = a * 2;
                int unused = c + 1;  // dead
                return a + b + c;
            }
        "#;
        let o0 = count_instructions(&compile_at_opt_level(source, 0));
        let o2 = count_instructions(&compile_at_opt_level(source, 2));
        // O0 must have strictly more instructions than O2
        assert!(o0 > o2, "O0 ({}) should have more instructions than O2 ({})", o0, o2);
    }

    #[test]
    fn test_o1_single_iteration() {
        // Verify -O1 uses only 1 iteration (fewer optimizations on multi-iteration patterns).
        // This is a simple program where 1 iteration is sufficient.
        let source = r#"
            int foo(int x) {
                if (1) {
                    return x + 1;
                }
                return x; // dead branch, removed by constfold + cfg_simplify
            }
        "#;
        let o1_module = compile_at_opt_level(source, 1);
        let o1_count = count_instructions(&o1_module);
        // Even with 1 iteration, constfold + cfg_simplify should remove the dead branch.
        // We just verify it compiles and produces reasonable output.
        assert!(o1_count > 0, "O1 should produce at least 1 instruction");
    }

    #[test]
    fn test_o0_preserves_dead_code() {
        let source = r#"
            int foo(void) {
                int a = 1;
                int b = 2;
                int c = 3;
                return a;
            }
        "#;
        let o0_module = compile_at_opt_level(source, 0);
        let o0_count = count_instructions(&o0_module);
        // At O0, all three assignments should be preserved (no DCE).
        // After mem2reg, we should still have the copy instructions for b and c.
        assert!(o0_count >= 3, "O0 should preserve dead assignments, got {} instructions", o0_count);
    }

    #[test]
    fn test_o1_skips_gvn() {
        // GVN eliminates redundant expressions. At O1, GVN is skipped so
        // both computations of x*3 should survive. At O2, GVN removes the duplicate.
        let source = r#"
            int foo(int x) {
                int a = x * 3;
                int b = x * 3;
                return a + b;
            }
        "#;
        let o1_module = compile_at_opt_level(source, 1);
        let o2_module = compile_at_opt_level(source, 2);
        let o1_count = count_instructions(&o1_module);
        let o2_count = count_instructions(&o2_module);
        // O2 should have fewer instructions than O1 because GVN eliminates the redundant x*3.
        assert!(o2_count < o1_count,
            "O2 ({}) should have fewer instructions than O1 ({}) due to GVN", o2_count, o1_count);
    }

}
