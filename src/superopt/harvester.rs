//! Harvest instruction patterns from CCC assembly output.
//!
//! Parses AT&T syntax assembly, splits into basic blocks, extracts
//! sliding windows, and prepares them for superoptimizer search.

use super::ir::*;
use super::cpu::OutputMask;
use super::sizing::program_size;
use std::collections::HashMap;

/// A harvested instruction window with metadata.
#[derive(Debug, Clone)]
pub struct HarvestedPattern {
    /// The instruction sequence.
    pub program: Program,
    /// How many times this exact pattern appeared.
    pub frequency: usize,
    /// Byte size of the pattern.
    pub byte_size: usize,
    /// Which registers are written by this window.
    pub regs_written: [bool; 8],
    /// Which registers are read after this window (estimated).
    pub regs_read_after: [bool; 8],
    /// Registers used in this window (for search operand pool).
    pub regs_used: Vec<u8>,
    /// Immediates used in this window.
    pub imms_used: Vec<i32>,
    /// Stack offsets used in this window.
    pub offsets_used: Vec<i32>,
}

/// Parse an AT&T syntax assembly file and extract instruction windows.
/// Returns deduplicated patterns sorted by potential savings (freq * size).
pub fn harvest_assembly(asm_text: &str, max_window: usize) -> Vec<HarvestedPattern> {
    let blocks = split_basic_blocks(asm_text);
    let mut pattern_counts: HashMap<String, (Program, usize)> = HashMap::new();

    for block in &blocks {
        // Extract sliding windows of sizes 2..=max_window.
        for window_size in 2..=max_window.min(block.len()) {
            for start in 0..=block.len() - window_size {
                let window: Vec<SInstr> = block[start..start + window_size].to_vec();
                let prog = Program::new(window);
                let key = prog.to_att();
                pattern_counts.entry(key)
                    .and_modify(|(_, count)| *count += 1)
                    .or_insert((prog, 1));
            }
        }
    }

    // Convert to HarvestedPattern with metadata.
    let mut patterns: Vec<HarvestedPattern> = Vec::new();
    for (_key, (program, frequency)) in pattern_counts {
        let byte_size = match program_size(&program) {
            Some(s) => s,
            None => continue,
        };

        let (regs_written, regs_read_after) = analyze_registers(&program);
        let (regs_used, imms_used, offsets_used) = collect_operands(&program);

        patterns.push(HarvestedPattern {
            program,
            frequency,
            byte_size,
            regs_written,
            regs_read_after,
            regs_used,
            imms_used,
            offsets_used,
        });
    }

    // Sort by potential impact: frequency * byte_size (largest first).
    patterns.sort_by(|a, b| {
        let impact_a = a.frequency * a.byte_size;
        let impact_b = b.frequency * b.byte_size;
        impact_b.cmp(&impact_a)
    });

    patterns
}

/// Split assembly text into basic blocks of parsed instructions.
/// Block boundaries: labels, jumps, calls, ret, directives.
fn split_basic_blocks(asm_text: &str) -> Vec<Vec<SInstr>> {
    let mut blocks: Vec<Vec<SInstr>> = Vec::new();
    let mut current_block: Vec<SInstr> = Vec::new();

    for line in asm_text.lines() {
        let trimmed = line.trim();

        // Skip empty lines.
        if trimmed.is_empty() {
            continue;
        }

        // Labels end the current block and start a new one.
        if trimmed.ends_with(':') {
            if !current_block.is_empty() {
                blocks.push(std::mem::take(&mut current_block));
            }
            continue;
        }

        // Directives (.globl, .cfi_*, .section, .size, .type, etc.) are block boundaries.
        if trimmed.starts_with('.') || trimmed.starts_with('#') {
            continue;
        }

        // Try to parse the instruction.
        if let Some(instr) = parse_att_instruction(trimmed) {
            // Control flow instructions end the current block.
            let is_control_flow = matches!(trimmed.split_whitespace().next(),
                Some(s) if s.starts_with('j') || s == "ret" || s == "call"
                    || s.starts_with("call"));

            if is_control_flow {
                if !current_block.is_empty() {
                    blocks.push(std::mem::take(&mut current_block));
                }
                // Don't add control flow instructions to blocks.
                continue;
            }

            current_block.push(instr);
        }
        // If we can't parse it, it's a block boundary (unknown instruction).
        else if !current_block.is_empty() {
            blocks.push(std::mem::take(&mut current_block));
        }
    }

    if !current_block.is_empty() {
        blocks.push(current_block);
    }

    blocks
}

/// Parse a single AT&T syntax instruction line.
/// Returns None for instructions we can't model.
pub fn parse_att_instruction(line: &str) -> Option<SInstr> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('.') {
        return None;
    }

    // Split mnemonic from operands.
    let (mnemonic_str, operands_str) = match trimmed.find(|c: char| c.is_whitespace()) {
        Some(pos) => (&trimmed[..pos], trimmed[pos..].trim()),
        None => (trimmed, ""),
    };

    let mnemonic = parse_mnemonic(mnemonic_str)?;

    if operands_str.is_empty() {
        // Zero-operand instruction.
        return Some(SInstr::new0(mnemonic));
    }

    // Split operands by comma (careful with parenthesized expressions).
    let ops = split_operands(operands_str);

    match ops.len() {
        1 => {
            let op0 = parse_operand(&ops[0])?;
            Some(SInstr::new1(mnemonic, op0))
        }
        2 => {
            let op0 = parse_operand(&ops[0])?;
            let op1 = parse_operand(&ops[1])?;
            Some(SInstr::new2(mnemonic, op0, op1))
        }
        3 => {
            let op0 = parse_operand(&ops[0])?;
            let op1 = parse_operand(&ops[1])?;
            let op2 = parse_operand(&ops[2])?;
            Some(SInstr::new3(mnemonic, op0, op1, op2))
        }
        _ => None,
    }
}

/// Parse a mnemonic string to our Mnemonic enum.
fn parse_mnemonic(s: &str) -> Option<Mnemonic> {
    match s {
        "movl" => Some(Mnemonic::Movl),
        "addl" => Some(Mnemonic::Addl),
        "subl" => Some(Mnemonic::Subl),
        "xorl" => Some(Mnemonic::Xorl),
        "andl" => Some(Mnemonic::Andl),
        "orl" => Some(Mnemonic::Orl),
        "cmpl" => Some(Mnemonic::Cmpl),
        "testl" => Some(Mnemonic::Testl),
        "negl" => Some(Mnemonic::Negl),
        "notl" => Some(Mnemonic::Notl),
        "incl" => Some(Mnemonic::Incl),
        "decl" => Some(Mnemonic::Decl),
        "shll" | "sall" => Some(Mnemonic::Shll),
        "shrl" => Some(Mnemonic::Shrl),
        "sarl" => Some(Mnemonic::Sarl),
        "leal" => Some(Mnemonic::Leal),
        "imull" => Some(Mnemonic::Imull),
        "movzbl" => Some(Mnemonic::Movzbl),
        "movsbl" => Some(Mnemonic::Movsbl),
        "cltd" | "cdq" => Some(Mnemonic::Cltd),
        "pushl" => Some(Mnemonic::Pushl),
        "popl" => Some(Mnemonic::Popl),
        "xchgl" => Some(Mnemonic::Xchgl),
        // Instructions we skip (control flow, byte ops, etc.)
        _ => None,
    }
}

/// Parse a single operand from AT&T syntax.
fn parse_operand(s: &str) -> Option<SOperand> {
    let s = s.trim();

    // Immediate: $value
    if let Some(rest) = s.strip_prefix('$') {
        let val = parse_integer(rest)?;
        return Some(SOperand::Imm(val));
    }

    // Register: %name
    if let Some(rest) = s.strip_prefix('%') {
        let reg = parse_register(rest)?;
        return Some(SOperand::Reg(reg));
    }

    // Memory: offset(%base) or offset(%base, %index, scale) or (%base)
    if let Some(paren_start) = s.find('(') {
        let offset_str = &s[..paren_start];
        let offset = if offset_str.is_empty() {
            0
        } else {
            parse_integer(offset_str)?
        };

        let inner = s[paren_start + 1..].strip_suffix(')')?;
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();

        match parts.len() {
            1 => {
                // offset(%base)
                let base = parse_register(parts[0].strip_prefix('%')?)?;
                // Treat ebp-relative and esp-relative as MemEbp for pattern matching.
                if base == REG_EBP || base == REG_ESP {
                    Some(SOperand::MemEbp(offset))
                } else {
                    // Memory through other register — can't model in superopt.
                    None
                }
            }
            3 => {
                // offset(%base, %index, scale)
                let base = parse_register(parts[0].strip_prefix('%')?)?;
                let index = parse_register(parts[1].strip_prefix('%')?)?;
                let scale: u8 = parts[2].parse().ok()?;
                if scale == 1 || scale == 2 || scale == 4 || scale == 8 {
                    Some(SOperand::LeaMem(base, index, scale, offset))
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        None
    }
}

/// Parse a register name to register index.
fn parse_register(s: &str) -> Option<u8> {
    match s {
        "eax" => Some(REG_EAX),
        "ecx" => Some(REG_ECX),
        "edx" => Some(REG_EDX),
        "ebx" => Some(REG_EBX),
        "esp" => Some(REG_ESP),
        "ebp" => Some(REG_EBP),
        "esi" => Some(REG_ESI),
        "edi" => Some(REG_EDI),
        // 8-bit register names — map to parent 32-bit register.
        "al" => Some(REG_EAX),
        "cl" => Some(REG_ECX),
        "dl" => Some(REG_EDX),
        "bl" => Some(REG_EBX),
        "ah" => Some(REG_EAX),
        "ch" => Some(REG_ECX),
        "dh" => Some(REG_EDX),
        "bh" => Some(REG_EBX),
        // 16-bit register names.
        "ax" => Some(REG_EAX),
        "cx" => Some(REG_ECX),
        "dx" => Some(REG_EDX),
        "bx" => Some(REG_EBX),
        "sp" => Some(REG_ESP),
        "bp" => Some(REG_EBP),
        "si" => Some(REG_ESI),
        "di" => Some(REG_EDI),
        _ => None,
    }
}

/// Parse an integer literal (decimal or hex).
fn parse_integer(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok().map(|v| v as i32)
    } else if let Some(hex) = s.strip_prefix("-0x").or_else(|| s.strip_prefix("-0X")) {
        u32::from_str_radix(hex, 16).ok().map(|v| -(v as i32))
    } else {
        s.parse::<i32>().ok()
    }
}

/// Split a comma-separated operand string, respecting parentheses.
fn split_operands(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;

    for c in s.chars() {
        match c {
            '(' => {
                paren_depth += 1;
                current.push(c);
            }
            ')' => {
                paren_depth -= 1;
                current.push(c);
            }
            ',' if paren_depth == 0 => {
                result.push(current.trim().to_string());
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }

    result
}

/// Analyze which registers a program reads and writes.
fn analyze_registers(program: &Program) -> ([bool; 8], [bool; 8]) {
    let mut written = [false; 8];
    let mut read = [false; 8];

    for instr in &program.instrs {
        match instr.mnemonic.num_operands() {
            0 => {
                // cltd: reads eax, writes edx.
                if instr.mnemonic == Mnemonic::Cltd {
                    read[REG_EAX as usize] = true;
                    written[REG_EDX as usize] = true;
                }
            }
            1 => {
                // Unary: operand is both read and written (except pushl which only reads).
                if let Some(r) = instr.operands[0].reg_id() {
                    read[r as usize] = true;
                    if instr.mnemonic != Mnemonic::Pushl {
                        written[r as usize] = true;
                    }
                }
            }
            2 | 3 => {
                // Source operand (0): read.
                collect_read_regs(&instr.operands[0], &mut read);
                // Destination operand (1): read + write for ALU, write-only for mov.
                if let Some(r) = instr.operands[1].reg_id() {
                    written[r as usize] = true;
                    // ALU ops also read the destination.
                    if instr.mnemonic != Mnemonic::Movl
                        && instr.mnemonic != Mnemonic::Leal
                        && instr.mnemonic != Mnemonic::Movzbl
                        && instr.mnemonic != Mnemonic::Movsbl
                    {
                        read[r as usize] = true;
                    }
                }
                // Memory in destination: base register is read.
                if let SOperand::MemEbp(_) = instr.operands[1] {
                    // EBP is implicitly read for addressing.
                }
            }
            _ => {}
        }
    }

    // Estimate what's "read after": conservatively assume all written registers
    // might be read later. In practice, we'd need post-window liveness analysis.
    (written, written)
}

fn collect_read_regs(op: &SOperand, read: &mut [bool; 8]) {
    match op {
        SOperand::Reg(r) => read[*r as usize] = true,
        SOperand::LeaMem(base, index, _, _) => {
            read[*base as usize] = true;
            read[*index as usize] = true;
        }
        _ => {}
    }
}

/// Collect all operand values used in a program (for building search pools).
fn collect_operands(program: &Program) -> (Vec<u8>, Vec<i32>, Vec<i32>) {
    let mut regs: Vec<u8> = Vec::new();
    let mut imms: Vec<i32> = Vec::new();
    let mut offsets: Vec<i32> = Vec::new();

    for instr in &program.instrs {
        for i in 0..instr.num_operands as usize {
            match instr.operands[i] {
                SOperand::Reg(r) => {
                    if !regs.contains(&r) {
                        regs.push(r);
                    }
                }
                SOperand::Imm(v) => {
                    if !imms.contains(&v) {
                        imms.push(v);
                    }
                }
                SOperand::MemEbp(off) => {
                    if !offsets.contains(&off) {
                        offsets.push(off);
                    }
                }
                SOperand::LeaMem(_, _, _, disp) => {
                    if !imms.contains(&disp) {
                        imms.push(disp);
                    }
                }
                SOperand::None => {}
            }
        }
    }

    // Always include common immediates and offsets for search diversity.
    for &v in &[0, 1, -1] {
        if !imms.contains(&v) {
            imms.push(v);
        }
    }

    (regs, imms, offsets)
}

/// Build an output mask for a harvested pattern.
/// Conservatively marks all written registers and relevant flags as outputs.
pub fn build_output_mask(pattern: &HarvestedPattern) -> OutputMask {
    let mut mask = OutputMask {
        regs: [false; 8],
        flags: false,
        stack_offsets: Vec::new(),
    };

    // All written registers must be preserved in the replacement.
    for i in 0..8 {
        if pattern.regs_written[i] {
            mask.regs[i] = true;
        }
    }

    // If no registers are written, mark EAX as output (conservative).
    if !mask.regs.iter().any(|&r| r) {
        mask.regs[REG_EAX as usize] = true;
    }

    // Detect if flags are a live output.
    // Walk backwards: if we find an instruction that sets flags and no later
    // instruction in the window overwrites flags, then flags survive to the
    // end of the window and may be read by a subsequent conditional jump.
    // Conservative: mark flags live if ANY flag-setter's flags reach the end.
    let mut flags_overwritten = false;
    for instr in pattern.program.instrs.iter().rev() {
        let sets_flags = matches!(instr.mnemonic,
            Mnemonic::Addl | Mnemonic::Subl | Mnemonic::Xorl | Mnemonic::Andl
            | Mnemonic::Orl | Mnemonic::Cmpl | Mnemonic::Testl | Mnemonic::Negl
            | Mnemonic::Incl | Mnemonic::Decl | Mnemonic::Shll | Mnemonic::Shrl
            | Mnemonic::Sarl | Mnemonic::Imull);

        if sets_flags && !flags_overwritten {
            // This instruction's flags survive to end of window.
            mask.flags = true;
            break;
        }

        if sets_flags {
            // Earlier flag-setter is overwritten by later one.
            flags_overwritten = true;
        }

        // Instructions that DON'T set flags: movl, leal, movzbl, movsbl,
        // notl, cltd, pushl, popl, xchgl. These don't overwrite flags.
    }

    // Include stack offsets that are written (memory side effects).
    for instr in &pattern.program.instrs {
        for i in 0..instr.num_operands as usize {
            // Detect memory writes (destination operand for 2-operand instructions).
            if i == 1 || (i == 0 && instr.num_operands == 1) {
                if let SOperand::MemEbp(off) = instr.operands[i] {
                    // Check if this is a write destination.
                    let is_write_dest = match instr.mnemonic {
                        Mnemonic::Movl | Mnemonic::Addl | Mnemonic::Subl
                        | Mnemonic::Xorl | Mnemonic::Andl | Mnemonic::Orl
                        | Mnemonic::Incl | Mnemonic::Decl | Mnemonic::Negl
                        | Mnemonic::Notl => i == 1 || instr.num_operands == 1,
                        _ => false,
                    };
                    if is_write_dest && !mask.stack_offsets.contains(&off) {
                        mask.stack_offsets.push(off);
                    }
                }
            }
        }
    }

    mask
}

/// Print a summary of harvested patterns.
pub fn print_harvest_summary(patterns: &[HarvestedPattern]) {
    let total_instructions: usize = patterns.iter().map(|p| p.program.len() * p.frequency).sum();
    let total_bytes: usize = patterns.iter().map(|p| p.byte_size * p.frequency).sum();
    let unique = patterns.len();

    eprintln!("=== Harvest Summary ===");
    eprintln!("Unique patterns:    {}", unique);
    eprintln!("Total occurrences:  {}", patterns.iter().map(|p| p.frequency).sum::<usize>());
    eprintln!("Total instructions: {}", total_instructions);
    eprintln!("Total bytes:        {}", total_bytes);
    eprintln!();

    // Show top 20 by impact.
    eprintln!("Top patterns by impact (frequency * size):");
    for (i, p) in patterns.iter().take(20).enumerate() {
        eprintln!(
            "  {:2}. [{:3}x] {:2} bytes  = {:4} total bytes  | {}",
            i + 1,
            p.frequency,
            p.byte_size,
            p.frequency * p.byte_size,
            p.program.to_att().replace('\n', " ; "),
        );
    }
}

// ── Cross-block harvesting ──────────────────────────────────────────────

/// A basic block with label and jump target info for cross-block analysis.
struct LabeledBlock {
    label: Option<String>,
    instrs: Vec<SInstr>,
    /// If block ends with unconditional `jmp .LABEL`, this holds the target label.
    jmp_target: Option<String>,
}

/// Parse assembly into labeled blocks, tracking jump targets.
fn parse_labeled_blocks(asm_text: &str) -> (Vec<LabeledBlock>, HashMap<String, usize>) {
    let mut blocks: Vec<LabeledBlock> = Vec::new();
    let mut current_label: Option<String> = None;
    let mut current_instrs: Vec<SInstr> = Vec::new();
    let mut label_map: HashMap<String, usize> = HashMap::new();

    let finish_block = |blocks: &mut Vec<LabeledBlock>,
                        label_map: &mut HashMap<String, usize>,
                        label: Option<String>,
                        instrs: Vec<SInstr>,
                        jmp_target: Option<String>| {
        let idx = blocks.len();
        if let Some(ref l) = label {
            label_map.insert(l.clone(), idx);
        }
        blocks.push(LabeledBlock { label, instrs, jmp_target });
    };

    for line in asm_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Labels start a new block.
        if trimmed.ends_with(':') && !trimmed.starts_with('.') ||
           (trimmed.ends_with(':') && trimmed.starts_with(".L")) {
            if !current_instrs.is_empty() || current_label.is_some() {
                let label = current_label.take();
                let instrs = std::mem::take(&mut current_instrs);
                finish_block(&mut blocks, &mut label_map, label, instrs, None);
            }
            current_label = Some(trimmed.trim_end_matches(':').to_string());
            continue;
        }

        // Directives — skip.
        if trimmed.starts_with('.') {
            continue;
        }

        // Check for control flow.
        let first_word = trimmed.split_whitespace().next().unwrap_or("");

        if first_word == "jmp" {
            // Unconditional jump — record target.
            let target = trimmed.strip_prefix("jmp").unwrap_or("").trim().to_string();
            let label = current_label.take();
            let instrs = std::mem::take(&mut current_instrs);
            finish_block(&mut blocks, &mut label_map, label, instrs, Some(target));
            continue;
        }

        if first_word.starts_with('j') || first_word == "ret" || first_word.starts_with("call") {
            // Other control flow — block boundary without recorded target.
            let label = current_label.take();
            let instrs = std::mem::take(&mut current_instrs);
            finish_block(&mut blocks, &mut label_map, label, instrs, None);
            continue;
        }

        // Regular instruction.
        if let Some(instr) = parse_att_instruction(trimmed) {
            current_instrs.push(instr);
        } else if !current_instrs.is_empty() {
            // Unparseable instruction = block boundary.
            let label = current_label.take();
            let instrs = std::mem::take(&mut current_instrs);
            finish_block(&mut blocks, &mut label_map, label, instrs, None);
        }
    }

    // Flush remaining.
    if !current_instrs.is_empty() || current_label.is_some() {
        let label = current_label;
        finish_block(&mut blocks, &mut label_map, label, current_instrs, None);
    }

    (blocks, label_map)
}

/// Harvest patterns including cross-block windows.
/// Follows unconditional jumps to create windows that span block boundaries.
/// This captures phi copy chains and store-reload patterns across jumps.
pub fn harvest_assembly_crossblock(asm_text: &str, max_window: usize) -> Vec<HarvestedPattern> {
    let (blocks, label_map) = parse_labeled_blocks(asm_text);
    let mut pattern_counts: HashMap<String, (Program, usize)> = HashMap::new();

    let mut add_windows = |instrs: &[SInstr], boundary: Option<usize>| {
        for window_size in 2..=max_window.min(instrs.len()) {
            for start in 0..=instrs.len() - window_size {
                // If boundary is set, only include windows that span it.
                if let Some(b) = boundary {
                    if start >= b || start + window_size <= b {
                        continue;
                    }
                }
                let window: Vec<SInstr> = instrs[start..start + window_size].to_vec();
                let prog = Program::new(window);
                let key = prog.to_att();
                pattern_counts.entry(key)
                    .and_modify(|(_, count)| *count += 1)
                    .or_insert((prog, 1));
            }
        }
    };

    // Phase 1: Within-block windows (same as regular harvester).
    for block in &blocks {
        add_windows(&block.instrs, None);
    }

    // Phase 2: Cross-block windows via unconditional jumps.
    for block in &blocks {
        if let Some(ref target) = block.jmp_target {
            if let Some(&target_idx) = label_map.get(target) {
                let target_block = &blocks[target_idx];
                // Build extended sequence: tail of source + head of target.
                let tail_len = max_window.min(block.instrs.len());
                let head_len = max_window.min(target_block.instrs.len());
                if tail_len == 0 || head_len == 0 {
                    continue;
                }
                let mut extended = Vec::with_capacity(tail_len + head_len);
                extended.extend_from_slice(&block.instrs[block.instrs.len() - tail_len..]);
                extended.extend_from_slice(&target_block.instrs[..head_len]);
                // Only extract windows that span the boundary.
                add_windows(&extended, Some(tail_len));
            }
        }
    }

    // Convert to HarvestedPattern with metadata (same as regular harvester).
    let mut patterns: Vec<HarvestedPattern> = Vec::new();
    for (_key, (program, frequency)) in pattern_counts {
        let byte_size = match program_size(&program) {
            Some(s) => s,
            None => continue,
        };
        let (regs_written, regs_read_after) = analyze_registers(&program);
        let (regs_used, imms_used, offsets_used) = collect_operands(&program);
        patterns.push(HarvestedPattern {
            program,
            frequency,
            byte_size,
            regs_written,
            regs_read_after,
            regs_used,
            imms_used,
            offsets_used,
        });
    }

    patterns.sort_by(|a, b| {
        let impact_a = a.frequency * a.byte_size;
        let impact_b = b.frequency * b.byte_size;
        impact_b.cmp(&impact_a)
    });

    patterns
}

/// Test if a pattern is an identity (no net state change).
/// Uses the emulator to check if the empty program produces the same outputs.
pub fn is_identity(pattern: &HarvestedPattern) -> bool {
    use super::verify::full_test;

    // Skip patterns with push/pop (emulator can't handle them).
    if pattern.program.instrs.iter().any(|i|
        matches!(i.mnemonic, Mnemonic::Pushl | Mnemonic::Popl)) {
        return false;
    }

    let empty = Program::new(vec![]);
    let mask = build_output_mask(pattern);
    full_test(&pattern.program, &empty, &mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_register_operand() {
        assert_eq!(parse_operand("%eax"), Some(SOperand::Reg(REG_EAX)));
        assert_eq!(parse_operand("%ecx"), Some(SOperand::Reg(REG_ECX)));
        assert_eq!(parse_operand("%edi"), Some(SOperand::Reg(REG_EDI)));
    }

    #[test]
    fn test_parse_immediate_operand() {
        assert_eq!(parse_operand("$42"), Some(SOperand::Imm(42)));
        assert_eq!(parse_operand("$-1"), Some(SOperand::Imm(-1)));
        assert_eq!(parse_operand("$0"), Some(SOperand::Imm(0)));
        assert_eq!(parse_operand("$255"), Some(SOperand::Imm(255)));
    }

    #[test]
    fn test_parse_memory_operand() {
        assert_eq!(parse_operand("-8(%ebp)"), Some(SOperand::MemEbp(-8)));
        assert_eq!(parse_operand("16(%esp)"), Some(SOperand::MemEbp(16)));
        assert_eq!(parse_operand("(%ebp)"), Some(SOperand::MemEbp(0)));
    }

    #[test]
    fn test_parse_instruction_reg_reg() {
        let instr = parse_att_instruction("    movl %eax, %ecx").unwrap();
        assert_eq!(instr.mnemonic, Mnemonic::Movl);
        assert_eq!(instr.operands[0], SOperand::Reg(REG_EAX));
        assert_eq!(instr.operands[1], SOperand::Reg(REG_ECX));
    }

    #[test]
    fn test_parse_instruction_imm_reg() {
        let instr = parse_att_instruction("    addl $1, %eax").unwrap();
        assert_eq!(instr.mnemonic, Mnemonic::Addl);
        assert_eq!(instr.operands[0], SOperand::Imm(1));
        assert_eq!(instr.operands[1], SOperand::Reg(REG_EAX));
    }

    #[test]
    fn test_parse_instruction_mem_reg() {
        let instr = parse_att_instruction("    movl -8(%ebp), %eax").unwrap();
        assert_eq!(instr.mnemonic, Mnemonic::Movl);
        assert_eq!(instr.operands[0], SOperand::MemEbp(-8));
        assert_eq!(instr.operands[1], SOperand::Reg(REG_EAX));
    }

    #[test]
    fn test_parse_instruction_unary() {
        let instr = parse_att_instruction("    incl %eax").unwrap();
        assert_eq!(instr.mnemonic, Mnemonic::Incl);
        assert_eq!(instr.operands[0], SOperand::Reg(REG_EAX));
    }

    #[test]
    fn test_parse_instruction_zero_operand() {
        let instr = parse_att_instruction("    cltd").unwrap();
        assert_eq!(instr.mnemonic, Mnemonic::Cltd);
        assert_eq!(instr.num_operands, 0);
    }

    #[test]
    fn test_parse_movzbl() {
        let instr = parse_att_instruction("    movzbl %al, %eax").unwrap();
        assert_eq!(instr.mnemonic, Mnemonic::Movzbl);
        assert_eq!(instr.operands[0], SOperand::Reg(REG_EAX));
        assert_eq!(instr.operands[1], SOperand::Reg(REG_EAX));
    }

    #[test]
    fn test_parse_returns_none_for_unknown() {
        assert!(parse_att_instruction("    ret").is_none());
        assert!(parse_att_instruction("    jmp .LBB1").is_none());
        assert!(parse_att_instruction("    movb %dl, (%ecx)").is_none());
    }

    #[test]
    fn test_split_basic_blocks() {
        let asm = "\
    movl %eax, %ecx
    addl $1, %ecx
.LBB1:
    subl %edx, %eax
    jmp .LBB2
    xorl %eax, %eax";
        let blocks = split_basic_blocks(asm);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].len(), 2); // movl, addl
        assert_eq!(blocks[1].len(), 1); // subl (jmp ends block)
        assert_eq!(blocks[2].len(), 1); // xorl
    }

    #[test]
    fn test_harvest_finds_patterns() {
        let asm = "\
    movl %eax, %ecx
    addl $1, %ecx
    movl %eax, %ecx
    addl $1, %ecx";
        let patterns = harvest_assembly(asm, 2);
        // Should find the 2-instruction window "movl %eax, %ecx; addl $1, %ecx" with frequency 2.
        let found = patterns.iter().find(|p| p.frequency == 2 && p.program.len() == 2);
        assert!(found.is_some(), "should find repeated 2-instruction pattern");
    }

    #[test]
    fn test_harvest_real_ccc_output() {
        // Fragment from actual CCC output.
        let asm = "\
    movl %ebx, %eax
    addl %edi, %eax
    movl %eax, %esi
    subl %ebp, %eax
    movl %eax, %edi
    movl %esi, %eax
    imull %edi, %eax
    addl %ebx, %eax";
        let patterns = harvest_assembly(asm, 3);
        assert!(!patterns.is_empty(), "should harvest patterns from real CCC output");
        // Check that we found some 2-instruction windows.
        let two_instr = patterns.iter().filter(|p| p.program.len() == 2).count();
        assert!(two_instr > 0);
    }

    #[test]
    fn test_parse_integer_hex() {
        assert_eq!(parse_integer("0xFF"), Some(0xFF));
        assert_eq!(parse_integer("-1"), Some(-1));
        assert_eq!(parse_integer("42"), Some(42));
    }

    #[test]
    fn test_split_operands_with_parens() {
        let ops = split_operands("-8(%ebp, %ecx, 4), %eax");
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0], "-8(%ebp, %ecx, 4)");
        assert_eq!(ops[1], "%eax");
    }
}
