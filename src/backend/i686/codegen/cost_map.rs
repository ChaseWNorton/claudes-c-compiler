//! Cost-map aggregation for --cost-map flag.
//!
//! Parses annotated assembly (instructions with `# CATEGORY` comment suffixes)
//! and produces per-function cost breakdowns showing where code size comes from.

/// Estimate the encoded byte size of an i686 instruction from its text.
fn estimate_instruction_size(line: &str) -> usize {
    let trimmed = line.trim();

    // Labels, directives, empty lines
    if trimmed.is_empty() || trimmed.starts_with('.') || trimmed.ends_with(':') {
        return 0;
    }

    // Split off any comment suffix (# CATEGORY)
    let instr = if let Some(idx) = trimmed.find('#') {
        trimmed[..idx].trim()
    } else {
        trimmed
    };

    if instr.is_empty() {
        return 0;
    }

    // Extract mnemonic (first word)
    let mnem = instr.split_whitespace().next().unwrap_or("");

    match mnem {
        // 1-byte instructions
        "ret" | "cltd" | "cdq" | "nop" | "int3" => 1,
        "pushl" | "popl" => {
            if instr.contains('%') {
                1 // push/pop register = 1 byte
            } else {
                // push immediate or memory
                if instr.contains('$') { 2 } else { 3 }
            }
        }
        // 2-byte instructions
        "xorl" | "testl" if instr.contains("%eax, %eax") => 2,
        "movl" => {
            if instr.contains("(%e") || instr.contains("(%esp)") {
                // movl mem, reg or movl reg, mem — typically 3-6 bytes
                4
            } else if instr.contains('$') {
                // movl $imm, %reg — 5 bytes (opcode + 4-byte imm)
                5
            } else {
                // movl %reg, %reg — 2 bytes
                2
            }
        }
        "addl" | "subl" | "andl" | "orl" | "xorl" | "cmpl" | "testl" => {
            if instr.contains('$') {
                if instr.contains("%esp") || instr.contains("%eax") {
                    // op $imm, %eax — shorter encoding, ~3-5 bytes
                    4
                } else {
                    5
                }
            } else if instr.contains("(%e") {
                4 // op mem, reg
            } else {
                2 // op %reg, %reg
            }
        }
        "imull" => {
            if instr.contains('$') { 6 } else { 3 }
        }
        "idivl" | "divl" => 2,
        "shll" | "shrl" | "sarl" => {
            if instr.contains('$') { 3 } else { 2 }
        }
        "leal" => 4,
        "call" => 5,
        "jmp" => {
            if instr.contains('*') { 2 } else { 5 } // indirect vs direct
        }
        s if s.starts_with('j') => 2, // jcc — short jump
        s if s.starts_with("set") => 3, // setcc %al
        "movzbl" | "movsbl" | "movzwl" | "movswl" => 3,
        "negl" | "notl" => 2,
        "bswapl" => 2,
        "lzcntl" | "tzcntl" => 4,
        _ => 3, // default estimate
    }
}

/// A single category's aggregated cost.
struct CategoryCost {
    tag: &'static str,
    bytes: usize,
    count: usize,
}

/// Parse annotated assembly and print cost breakdown to stderr.
pub fn print_cost_map(asm: &str) {
    let mut current_func: Option<String> = None;
    let mut func_costs: Vec<(String, Vec<CategoryCost>)> = Vec::new();
    let mut current_costs: Vec<(& str, usize)> = Vec::new(); // (tag, estimated_bytes)

    for line in asm.lines() {
        let trimmed = line.trim();

        // Detect function labels (non-local labels that start a function)
        if !trimmed.is_empty() && !trimmed.starts_with('.') && !trimmed.starts_with(' ')
            && !trimmed.starts_with('\t') && trimmed.ends_with(':')
        {
            // Flush previous function
            if let Some(func_name) = current_func.take() {
                let costs = aggregate_costs(&current_costs);
                if !costs.is_empty() {
                    func_costs.push((func_name, costs));
                }
                current_costs.clear();
            }
            current_func = Some(trimmed.trim_end_matches(':').to_string());
            continue;
        }

        // Skip directives, empty lines
        if trimmed.is_empty() || trimmed.starts_with('.') || trimmed.ends_with(':') {
            continue;
        }

        // Parse instruction with optional # TAG suffix
        let (tag, size) = if let Some(hash_idx) = trimmed.rfind("    # ") {
            let tag_str = &trimmed[hash_idx + 6..];
            let size = estimate_instruction_size(&trimmed[..hash_idx]);
            (tag_str, size)
        } else {
            let size = estimate_instruction_size(trimmed);
            ("OTHER", size)
        };

        if size > 0 {
            current_costs.push((tag, size));
        }
    }

    // Flush last function
    if let Some(func_name) = current_func.take() {
        let costs = aggregate_costs(&current_costs);
        if !costs.is_empty() {
            func_costs.push((func_name, costs));
        }
    }

    if func_costs.is_empty() {
        return;
    }

    // Sort functions by total cost descending
    func_costs.sort_by(|a, b| {
        let a_total: usize = a.1.iter().map(|c| c.bytes).sum();
        let b_total: usize = b.1.iter().map(|c| c.bytes).sum();
        b_total.cmp(&a_total)
    });

    // Print report
    eprintln!("\n=== Cost Map ===");
    let mut grand_total = 0usize;
    let mut grand_by_tag: Vec<(&str, usize)> = Vec::new();

    for (func_name, costs) in &func_costs {
        let total: usize = costs.iter().map(|c| c.bytes).sum();
        grand_total += total;
        eprintln!("\n{} (~{} bytes):", func_name, total);
        for c in costs {
            let pct = if total > 0 { (c.bytes as f64 / total as f64) * 100.0 } else { 0.0 };
            eprintln!("  {:12} {:5} bytes ({:5.1}%)  [{} instr]", c.tag, c.bytes, pct, c.count);
            // Accumulate grand totals
            if let Some(entry) = grand_by_tag.iter_mut().find(|(t, _)| *t == c.tag) {
                entry.1 += c.bytes;
            } else {
                grand_by_tag.push((c.tag, c.bytes));
            }
        }
    }

    // Grand summary
    grand_by_tag.sort_by(|a, b| b.1.cmp(&a.1));
    eprintln!("\n--- Grand Total: ~{} bytes across {} functions ---", grand_total, func_costs.len());
    for (tag, bytes) in &grand_by_tag {
        let pct = if grand_total > 0 { (*bytes as f64 / grand_total as f64) * 100.0 } else { 0.0 };
        eprintln!("  {:12} {:5} bytes ({:5.1}%)", tag, bytes, pct);
    }
    eprintln!();
}

fn aggregate_costs(entries: &[(&str, usize)]) -> Vec<CategoryCost> {
    let tags = [
        "RELOAD", "SPILL", "COMPUTE", "ACCUM_IN", "ACCUM_OUT",
        "PHI_COPY", "ARG_COPY", "CALL_SETUP", "CALL", "PROLOGUE",
        "BRANCH", "LOAD_ARG", "OTHER",
    ];

    let mut result = Vec::new();
    for &tag in &tags {
        let (bytes, count) = entries.iter()
            .filter(|(t, _)| *t == tag)
            .fold((0usize, 0usize), |(b, c), (_, sz)| (b + sz, c + 1));
        if count > 0 {
            result.push(CategoryCost { tag, bytes, count });
        }
    }

    // Catch any unrecognized tags
    for (tag, sz) in entries {
        if !tags.contains(tag) {
            if let Some(c) = result.iter_mut().find(|c| c.tag == "OTHER") {
                c.bytes += sz;
                c.count += 1;
            } else {
                result.push(CategoryCost { tag: "OTHER", bytes: *sz, count: 1 });
            }
        }
    }

    // Sort by bytes descending
    result.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    result
}
