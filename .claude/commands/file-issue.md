Discover bugs in the CCC codebase and file well-structured GitHub issues.

## Instructions

If `$ARGUMENTS` is provided, audit that specific file or module. Otherwise, pick the most impactful area to audit.

### Step 1: Choose what to audit

Good audit targets (highest impact first):
- `src/frontend/sema/analysis.rs` — missing diagnostics
- `src/driver/cli.rs` — silent CLI failures
- `src/frontend/lexer/scan.rs` — missing validation
- `src/backend/*/codegen.rs` — incorrect codegen
- `src/common/types.rs` — type system gaps

### Step 2: Read the code

Read the target file(s) thoroughly. Look for:
- Missing error handling (silent failures, `unwrap_or`, ignored errors)
- Missing diagnostics (conditions GCC/Clang would flag)
- TODO comments indicating known gaps
- Incorrect logic (compare against C11 standard behavior)

### Step 3: Write reproduction cases

For each bug found, write a minimal C program that demonstrates it:
```c
// This should produce an error but doesn't
int main() { /* trigger code */ }
```

### Step 4: Check for duplicates

```bash
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50
```

Don't file an issue if one already exists for the same problem.

### Step 5: File each issue

Use the issue template format from the file-issue skill. Each issue must be a complete work order with: Problem, Expected behavior, Reproduction, Suggested approach, Files to modify, Tests to write, Acceptance criteria.

Assign priority: [P0] critical, [P1] high, [P2] medium, [P3] low.

### Step 6: Report

For each issue filed, report the issue number, title, and priority.
