# Audit — Deep Codebase Analysis

Systematically audit a module or area of the CCC codebase, discover every gap,
and file structured issues for each one.

## When to Use

Use this skill when:
- You want to find all issues in a specific module (not just one bug)
- You're doing a quality pass before a milestone
- You want to expand the issue backlog with real, discovered problems
- A module has been flagged as under-tested or under-reviewed

## How It Differs from /file-issue

| | `/file-issue` | `/audit` |
|---|---|---|
| Scope | Single bug you already found | Entire module, find everything |
| Approach | Reactive — you know the problem | Systematic — discover problems |
| Output | 1-3 issues | 5-20 issues |
| When | You stumbled on a bug | Planned quality sweep |

## Audit Workflow

### 1. Choose the audit target

Pick one of these areas (or use `$ARGUMENTS` if provided):

**High-value audit targets:**

| Area | Files | What to look for |
|------|-------|-----------------|
| Sema pass | `src/frontend/sema/analysis.rs` | Missing diagnostics, incomplete type checking |
| Lexer | `src/frontend/lexer/scan.rs` | Missing validation, incorrect tokenization |
| Parser | `src/frontend/parser/` | Missing error recovery, silent parse failures |
| CLI | `src/driver/cli.rs` | Silent failures, missing validation |
| Type system | `src/common/types.rs` | Incomplete comparisons, missing qualifiers |
| x86-64 codegen | `src/backend/x86/codegen.rs` | Wrong instructions, ABI violations |
| i686 codegen | `src/backend/i686/codegen.rs` | 32-bit specific issues |
| ARM codegen | `src/backend/arm/codegen.rs` | ARM-specific issues |
| RISC-V codegen | `src/backend/riscv/codegen.rs` | RISC-V specific issues |
| Optimizer | `src/passes/` | Incorrect transformations, missed optimizations |
| IR lowering | `src/ir/lowering/` | Incorrect lowering, missing cases |

### 2. Read everything in the target

Read every file in the module. Don't skim — read line by line. Look for:

**Code smells:**
- `unwrap()` / `unwrap_or()` on user input (should be error)
- `// TODO` or `// FIXME` comments
- `unreachable!()` that might actually be reachable
- Empty match arms or catch-all `_ =>` that swallow cases
- Silent fallthrough (condition not checked, default assumed)

**Missing diagnostics:**
- Compare against GCC/Clang behavior for the same C construct
- Check the C11 standard section for required diagnostics
- Look for constraints that are parsed but not validated

**Incorrect behavior:**
- Type mismatches that aren't caught
- Calculations that could overflow
- Off-by-one errors in array/struct layout
- Wrong register or instruction selection (backends)

**Missing tests:**
- Functions with no test coverage
- Edge cases not tested (empty input, max values, nested constructs)
- Error paths not tested

### 3. Write reproduction cases

For each issue found, write minimal C code that demonstrates it:

```c
// Should produce an error but doesn't
int main() {
    switch(0) {
        case 1: break;
        case 1: break;  // duplicate — GCC errors, CCC silent
    }
}
```

Compile with CCC and verify the bug exists. Compare with GCC output.

### 4. Check for duplicates

```bash
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50
```

Skip anything that already has an open issue.

### 5. File issues

File each discovered issue using the standard template. Include:
- Priority prefix `[P0]`-`[P3]`
- Reproduction case
- Suggested approach with specific file and line references
- Test cases to write
- Link to milestone if applicable: `Part of [MILESTONE] M<N> (#<NUMBER>)`

### 6. Report

```
AUDIT REPORT: <module name>
============================

Files read: X
Issues found: Y (X new, Y already tracked)
Issues filed: Z

New issues:
  #XX [P0] <title>
  #XX [P1] <title>
  #XX [P2] <title>
  ...

Already tracked:
  #XX <title> (existing issue)
  ...

Recommendations:
  - <any structural observations about the module>
  - <suggestions for milestones>
```

## Audit Checklist

For each file in the audit target, check:

- [ ] Read every function
- [ ] Identified all `unwrap()` / `unwrap_or()` on external input
- [ ] Checked all TODO/FIXME comments
- [ ] Compared diagnostic coverage against GCC behavior
- [ ] Looked for missing error handling
- [ ] Checked test coverage (is there a `#[cfg(test)]` module?)
- [ ] Written reproduction cases for each issue
- [ ] Filed issues for all new findings

## Reference Files

- **[AUDIT_AREAS.md](AUDIT_AREAS.md)** — Detailed guide for each auditable area
