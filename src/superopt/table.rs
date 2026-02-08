//! Serialization and code generation for rewrite rules.
//!
//! Handles reading/writing rules to text format and generating
//! Rust source code for the lookup table.

use super::search::RewriteRule;

/// Serialize rules to a human-readable text format.
pub fn write_rules_text(rules: &[RewriteRule]) -> String {
    let mut out = String::new();
    for rule in rules {
        out.push_str(&format!(
            "# saves {} bytes ({} -> {})\n",
            rule.savings(),
            rule.original_size,
            rule.replacement_size,
        ));
        out.push_str("ORIGINAL:\n");
        for instr in &rule.original.instrs {
            out.push_str(&instr.to_att());
            out.push('\n');
        }
        out.push_str("REPLACEMENT:\n");
        if rule.replacement.is_empty() {
            out.push_str("  (empty)\n");
        } else {
            for instr in &rule.replacement.instrs {
                out.push_str(&instr.to_att());
                out.push('\n');
            }
        }
        out.push_str("---\n");
    }
    out
}

/// Print a summary of discovered rules.
pub fn print_summary(rules: &[RewriteRule]) {
    let total_savings: usize = rules.iter().map(|r| r.savings()).sum();
    eprintln!("=== Superoptimizer Results ===");
    eprintln!("Rules found:   {}", rules.len());
    eprintln!("Total savings: {} bytes (per occurrence)", total_savings);
    eprintln!();
    for (i, rule) in rules.iter().enumerate() {
        eprintln!(
            "Rule {}: saves {} bytes ({} -> {})",
            i + 1,
            rule.savings(),
            rule.original_size,
            rule.replacement_size,
        );
        eprintln!("  Before: {}", rule.original);
        eprintln!("  After:  {}", if rule.replacement.is_empty() {
            "(deleted)".to_string()
        } else {
            rule.replacement.to_string()
        });
    }
}
