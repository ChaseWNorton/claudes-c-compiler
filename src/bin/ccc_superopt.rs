//! CCC Superoptimizer — finds optimal instruction replacements via exhaustive search.
//!
//! Usage:
//!   ccc-superopt demo                    — Run demo search on known-optimizable patterns
//!   ccc-superopt harvest <file>          — Harvest patterns from assembly file, show summary
//!   ccc-superopt search <file> [N]       — Search for optimal replacements (window size N, default 3)
//!   ccc-superopt diagnose <file> [...]   — Diagnostic report: ranked hitlist of optimization targets
//!   ccc-superopt validate                — Re-verify rules with extra test vectors

use ccc::superopt::ir::*;
use ccc::superopt::cpu::OutputMask;
use ccc::superopt::search;
use ccc::superopt::table;
use ccc::superopt::harvester;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("demo") => run_demo(),
        Some("harvest") => {
            let file = args.get(2).unwrap_or_else(|| {
                eprintln!("Usage: ccc-superopt harvest <assembly-file>");
                std::process::exit(1);
            });
            run_harvest(file);
        }
        Some("search") => {
            let file = args.get(2).unwrap_or_else(|| {
                eprintln!("Usage: ccc-superopt search <assembly-file>");
                std::process::exit(1);
            });
            let max_window = args.get(3)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(3);
            run_search(file, max_window);
        }
        Some("diagnose") => {
            if args.len() < 3 {
                eprintln!("Usage: ccc-superopt diagnose <file1.s> [file2.s ...]");
                std::process::exit(1);
            }
            run_diagnose(&args[2..]);
        }
        Some("validate") => {
            eprintln!("Validate mode not yet implemented.");
            std::process::exit(1);
        }
        _ => {
            eprintln!("CCC Superoptimizer");
            eprintln!();
            eprintln!("Usage: ccc-superopt <command> [args]");
            eprintln!();
            eprintln!("Commands:");
            eprintln!("  demo                    Run demo search on known-optimizable patterns");
            eprintln!("  harvest <file>          Analyze assembly file, show pattern summary");
            eprintln!("  search <file> [N]       Search for optimal replacements (window size N, default 3)");
            eprintln!("  diagnose <file> [...]   Diagnostic: ranked hitlist across multiple files");
            eprintln!("  validate                Re-verify discovered rules (future)");
            std::process::exit(1);
        }
    }
}

/// Harvest mode: read assembly file, extract patterns, show summary.
fn run_harvest(file: &str) {
    let asm_text = std::fs::read_to_string(file).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", file, e);
        std::process::exit(1);
    });

    eprintln!("=== CCC Superoptimizer: Harvest ===");
    eprintln!("Input: {}", file);
    eprintln!();

    let patterns = harvester::harvest_assembly(&asm_text, 4);
    harvester::print_harvest_summary(&patterns);
}

/// Search mode: harvest patterns from assembly, then search for optimizations.
fn run_search(file: &str, max_window: usize) {
    let asm_text = std::fs::read_to_string(file).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", file, e);
        std::process::exit(1);
    });

    eprintln!("=== CCC Superoptimizer: Search ===");
    eprintln!("Input: {}", file);
    eprintln!("Max window: {} instructions", max_window);
    eprintln!();

    let patterns = harvester::harvest_assembly(&asm_text, max_window);
    harvester::print_harvest_summary(&patterns);
    eprintln!();

    // Convert harvested patterns to search format.
    let mut search_patterns: Vec<(Program, OutputMask, Vec<u8>, Vec<i32>, Vec<i32>)> = Vec::new();

    for pattern in &patterns {
        // Skip patterns that are already very small (1-2 bytes) — nothing shorter.
        if pattern.byte_size <= 2 {
            continue;
        }

        // Skip patterns with push/pop (emulator can't handle them).
        let has_pushpop = pattern.program.instrs.iter().any(|i|
            matches!(i.mnemonic, Mnemonic::Pushl | Mnemonic::Popl));
        if has_pushpop {
            continue;
        }

        let mask = harvester::build_output_mask(pattern);
        search_patterns.push((
            pattern.program.clone(),
            mask,
            pattern.regs_used.clone(),
            pattern.imms_used.clone(),
            pattern.offsets_used.clone(),
        ));
    }

    eprintln!("Searching {} patterns...", search_patterns.len());
    eprintln!();

    let rules = search::search_patterns(&search_patterns);

    if rules.is_empty() {
        eprintln!("No optimizations found.");
        return;
    }

    // Calculate total real savings.
    let mut total_savings = 0usize;
    for rule in &rules {
        // Find the harvested pattern to get frequency.
        let freq = patterns.iter()
            .find(|p| p.program == rule.original)
            .map(|p| p.frequency)
            .unwrap_or(1);
        total_savings += rule.savings() * freq;
    }

    eprintln!();
    table::print_summary(&rules);
    eprintln!();
    eprintln!("=== Projected Real Savings ===");
    for rule in &rules {
        let freq = patterns.iter()
            .find(|p| p.program == rule.original)
            .map(|p| p.frequency)
            .unwrap_or(1);
        eprintln!(
            "  {} -> {} : saves {} bytes x {} occurrences = {} bytes",
            rule.original.to_att().replace('\n', " ; "),
            if rule.replacement.is_empty() { "(deleted)".to_string() }
            else { rule.replacement.to_att().replace('\n', " ; ") },
            rule.savings(),
            freq,
            rule.savings() * freq,
        );
    }
    eprintln!();
    eprintln!("TOTAL PROJECTED SAVINGS: {} bytes", total_savings);

    // Print text format to stdout.
    print!("{}", table::write_rules_text(&rules));
}

/// Diagnose mode: cross-block analysis of multiple assembly files.
/// Outputs a ranked hitlist of optimization targets.
fn run_diagnose(files: &[String]) {
    use std::collections::HashMap;

    eprintln!("=== CCC Superoptimizer: Diagnose ===");
    eprintln!("Files: {}", files.len());
    for f in files {
        eprintln!("  {}", f);
    }
    eprintln!();

    // Read and concatenate all assembly files.
    // Also track per-file content for per-file breakdown later.
    let mut all_asm = String::new();
    let mut file_contents: Vec<(String, String)> = Vec::new();
    for f in files {
        match std::fs::read_to_string(f) {
            Ok(content) => {
                all_asm.push_str(&content);
                all_asm.push('\n');
                file_contents.push((f.clone(), content));
            }
            Err(e) => {
                eprintln!("Warning: cannot read {}: {}", f, e);
            }
        }
    }

    // Phase 1: Cross-block harvest (window size 4 for cross-block patterns).
    eprintln!("Phase 1: Cross-block harvest (window size 4)...");
    let patterns = harvester::harvest_assembly_crossblock(&all_asm, 4);
    eprintln!("  {} unique patterns harvested", patterns.len());
    eprintln!();

    // Phase 2: Identify identity patterns (no net state change).
    eprintln!("Phase 2: Identity detection...");
    let mut identities: Vec<&harvester::HarvestedPattern> = Vec::new();
    let mut identity_bytes = 0usize;
    let mut checked = 0;
    for pattern in &patterns {
        // Only check patterns worth examining (freq*size > threshold).
        if pattern.frequency * pattern.byte_size < 6 {
            continue;
        }
        // Skip single-instruction patterns (too small to be meaningful identities).
        if pattern.program.len() < 2 {
            continue;
        }
        checked += 1;
        if harvester::is_identity(pattern) {
            let waste = pattern.frequency * pattern.byte_size;
            identity_bytes += waste;
            identities.push(pattern);
        }
    }
    eprintln!("  Checked {} patterns, found {} identities ({} bytes total waste)",
        checked, identities.len(), identity_bytes);
    eprintln!();

    // Phase 3: Search for shorter replacements on top non-identity patterns.
    eprintln!("Phase 3: Searching top patterns for shorter replacements...");
    let top_n = 50; // search the top 50 patterns by impact
    let mut search_count = 0;
    let mut optimizable: Vec<(search::RewriteRule, usize)> = Vec::new(); // (rule, frequency)

    for pattern in &patterns {
        if search_count >= top_n {
            break;
        }
        // Skip tiny, identity, or push/pop patterns.
        if pattern.byte_size <= 2 {
            continue;
        }
        if pattern.program.instrs.iter().any(|i|
            matches!(i.mnemonic, Mnemonic::Pushl | Mnemonic::Popl)) {
            continue;
        }
        // Skip patterns already identified as identities.
        if identities.iter().any(|id| id.program == pattern.program) {
            continue;
        }

        let mask = harvester::build_output_mask(pattern);
        search_count += 1;
        if let Some(rule) = search::find_optimal(
            &pattern.program,
            &mask,
            &pattern.regs_used,
            &pattern.imms_used,
            &pattern.offsets_used,
        ) {
            optimizable.push((rule, pattern.frequency));
        }
    }

    // Sort results.
    identities.sort_by(|a, b| {
        (b.frequency * b.byte_size).cmp(&(a.frequency * a.byte_size))
    });
    optimizable.sort_by(|a, b| {
        let savings_a = a.0.savings() * a.1;
        let savings_b = b.0.savings() * b.1;
        savings_b.cmp(&savings_a)
    });

    // ── Report ──────────────────────────────────────────────────────────

    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════╗");
    eprintln!("║                  DIAGNOSTIC REPORT                              ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════╝");
    eprintln!();

    // Category 1: Identity patterns (no-ops that can be deleted entirely).
    if !identities.is_empty() {
        eprintln!("=== IDENTITY PATTERNS (no net effect — delete entirely) ===");
        eprintln!("{:>4}  {:>4}  {:>5}  {:>6}  Pattern", "Rank", "Freq", "Bytes", "Waste");
        eprintln!("{}", "-".repeat(70));
        for (i, pattern) in identities.iter().take(20).enumerate() {
            let waste = pattern.frequency * pattern.byte_size;
            eprintln!(
                "{:>4}  {:>4}x {:>3}B   {:>5}B  {}",
                i + 1,
                pattern.frequency,
                pattern.byte_size,
                waste,
                pattern.program.to_att().replace('\n', " ; "),
            );
        }
        eprintln!();
        eprintln!("Identity total: {} patterns, {} bytes wasted", identities.len(), identity_bytes);
        eprintln!();
    }

    // Category 2: Optimizable patterns (shorter replacement exists).
    let mut opt_total_savings = 0usize;
    if !optimizable.is_empty() {
        eprintln!("=== OPTIMIZABLE PATTERNS (shorter replacement found) ===");
        eprintln!("{:>4}  {:>4}  {:>5}  {:>6}  Pattern -> Replacement", "Rank", "Freq", "Saves", "Total");
        eprintln!("{}", "-".repeat(80));
        for (i, (rule, freq)) in optimizable.iter().take(20).enumerate() {
            let total = rule.savings() * freq;
            opt_total_savings += total;
            let replacement = if rule.replacement.is_empty() {
                "(deleted)".to_string()
            } else {
                rule.replacement.to_att().replace('\n', " ; ")
            };
            eprintln!(
                "{:>4}  {:>4}x {:>3}B   {:>5}B  {} -> {}",
                i + 1,
                freq,
                rule.savings(),
                total,
                rule.original.to_att().replace('\n', " ; "),
                replacement,
            );
        }
        eprintln!();
        eprintln!("Optimizable total: {} patterns, {} bytes saveable", optimizable.len(), opt_total_savings);
        eprintln!();
    }

    // Per-file breakdown.
    eprintln!("=== PER-FILE BREAKDOWN ===");
    let mut per_file_identity: HashMap<String, usize> = HashMap::new();
    for (filename, content) in &file_contents {
        let mut file_waste = 0usize;
        for id_pattern in &identities {
            // Count occurrences of this identity pattern in this file.
            let file_patterns = harvester::harvest_assembly_crossblock(content, 4);
            for fp in &file_patterns {
                if fp.program == id_pattern.program {
                    file_waste += fp.frequency * fp.byte_size;
                }
            }
        }
        let basename = std::path::Path::new(filename)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if file_waste > 0 {
            per_file_identity.insert(basename.to_string(), file_waste);
        }
    }
    let mut file_list: Vec<_> = per_file_identity.iter().collect();
    file_list.sort_by(|a, b| b.1.cmp(a.1));
    for (fname, waste) in &file_list {
        eprintln!("  {:30} ~{}B from identity patterns", fname, waste);
    }
    eprintln!();

    // Grand total.
    let grand_total = identity_bytes + opt_total_savings;
    eprintln!("╔══════════════════════════════════════════════════════════════════╗");
    eprintln!("║  TOTAL PROJECTED SAVINGS: {:>6} bytes                          ║", grand_total);
    eprintln!("║    Identity patterns:     {:>6} bytes                          ║", identity_bytes);
    eprintln!("║    Optimizable patterns:  {:>6} bytes                          ║", opt_total_savings);
    eprintln!("╚══════════════════════════════════════════════════════════════════╝");

    // Print machine-readable rules to stdout.
    if !optimizable.is_empty() {
        println!();
        println!("# Rewrite rules discovered by diagnose mode");
        let rules: Vec<_> = optimizable.iter().map(|(r, _)| r.clone()).collect();
        print!("{}", table::write_rules_text(&rules));
    }
}

/// Demo mode: search for optimal replacements for well-known patterns.
fn run_demo() {
    eprintln!("=== CCC Superoptimizer Demo ===");
    eprintln!();

    let common_regs = &[REG_EAX, REG_ECX, REG_EDX];
    let common_imms = &[0, 1, -1, 2, 3, 4, 8, 16, 31];
    let common_offsets: &[i32] = &[-8, -12, -16, -4];

    let patterns: Vec<(Program, OutputMask, Vec<u8>, Vec<i32>, Vec<i32>)> = vec![
        // 1. movl $0, %eax → xorl %eax, %eax (saves 3 bytes)
        (
            Program::new(vec![
                SInstr::new2(Mnemonic::Movl, SOperand::Imm(0), SOperand::Reg(REG_EAX)),
            ]),
            OutputMask::regs_and_flags(&[REG_EAX], false),
            common_regs.to_vec(),
            common_imms.to_vec(),
            common_offsets.to_vec(),
        ),
        // 2. movl $0, %ecx → xorl %ecx, %ecx (saves 3 bytes)
        (
            Program::new(vec![
                SInstr::new2(Mnemonic::Movl, SOperand::Imm(0), SOperand::Reg(REG_ECX)),
            ]),
            OutputMask::regs_and_flags(&[REG_ECX], false),
            common_regs.to_vec(),
            common_imms.to_vec(),
            common_offsets.to_vec(),
        ),
        // 3. addl $1, %eax → incl %eax (saves 2 bytes)
        (
            Program::new(vec![
                SInstr::new2(Mnemonic::Addl, SOperand::Imm(1), SOperand::Reg(REG_EAX)),
            ]),
            OutputMask::regs_and_flags(&[REG_EAX], false),
            common_regs.to_vec(),
            common_imms.to_vec(),
            common_offsets.to_vec(),
        ),
        // 4. subl $1, %eax → decl %eax (saves 2 bytes)
        (
            Program::new(vec![
                SInstr::new2(Mnemonic::Subl, SOperand::Imm(1), SOperand::Reg(REG_EAX)),
            ]),
            OutputMask::regs_and_flags(&[REG_EAX], false),
            common_regs.to_vec(),
            common_imms.to_vec(),
            common_offsets.to_vec(),
        ),
        // 5. movl %eax, %ecx; movl %edx, %ecx → movl %edx, %ecx (dead code)
        (
            Program::new(vec![
                SInstr::new2(Mnemonic::Movl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_ECX)),
                SInstr::new2(Mnemonic::Movl, SOperand::Reg(REG_EDX), SOperand::Reg(REG_ECX)),
            ]),
            OutputMask::regs_and_flags(&[REG_ECX], false),
            common_regs.to_vec(),
            common_imms.to_vec(),
            common_offsets.to_vec(),
        ),
        // 6. addl $0, %eax → (empty, no-op)
        (
            Program::new(vec![
                SInstr::new2(Mnemonic::Addl, SOperand::Imm(0), SOperand::Reg(REG_EAX)),
            ]),
            OutputMask::regs_and_flags(&[REG_EAX], false),
            common_regs.to_vec(),
            common_imms.to_vec(),
            common_offsets.to_vec(),
        ),
        // 7. movl %eax, -8(%ebp); movl -8(%ebp), %ecx → movl %eax, %ecx
        (
            Program::new(vec![
                SInstr::new2(Mnemonic::Movl, SOperand::Reg(REG_EAX), SOperand::MemEbp(-8)),
                SInstr::new2(Mnemonic::Movl, SOperand::MemEbp(-8), SOperand::Reg(REG_ECX)),
            ]),
            OutputMask::regs_and_flags(&[REG_ECX], false),
            common_regs.to_vec(),
            common_imms.to_vec(),
            common_offsets.to_vec(),
        ),
        // 8. subl $-1, %eax → incl %eax
        (
            Program::new(vec![
                SInstr::new2(Mnemonic::Subl, SOperand::Imm(-1), SOperand::Reg(REG_EAX)),
            ]),
            OutputMask::regs_and_flags(&[REG_EAX], false),
            common_regs.to_vec(),
            common_imms.to_vec(),
            common_offsets.to_vec(),
        ),
    ];

    let rules = search::search_patterns(&patterns);
    eprintln!();
    table::print_summary(&rules);
    print!("{}", table::write_rules_text(&rules));
}
