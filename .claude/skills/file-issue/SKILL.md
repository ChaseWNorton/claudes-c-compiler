# File Issue — Bug Discovery and Issue Creation

Analyze the CCC codebase to discover bugs, then file well-structured GitHub issues that serve as complete work orders.

## When to Use

Use this skill when:
- You've found a bug during code review or testing
- You want to systematically audit a module for issues
- You want to expand the issue backlog with new work

## Discovery Methods

### Method 1: Targeted audit

Pick a module and read it looking for problems:

```
1. Read the source file(s)
2. Look for: missing error handling, silent failures, incorrect logic, TODO comments
3. Compare behavior against the C11 standard or GCC/Clang output
4. Write a minimal C program that triggers each issue
5. File an issue for each confirmed bug
```

Good audit targets:
- `src/frontend/sema/analysis.rs` — missing diagnostics
- `src/driver/cli.rs` — missing argument validation
- `src/frontend/lexer/scan.rs` — missing validation (unicode, escape sequences)
- `src/backend/*/codegen.rs` — incorrect instruction generation
- `src/common/types.rs` — type system gaps

### Method 2: Standard compliance check

Pick a C11 standard section and verify CCC implements it correctly:

```
1. Read the relevant section (e.g., C11 6.7.3 Type qualifiers)
2. Write test programs that exercise the specified behavior
3. Compile with CCC and with GCC/Clang
4. Compare diagnostics and behavior
5. File issues for any differences
```

### Method 3: Test coverage analysis

Look for code paths with zero test coverage:

```
1. Read a module's test section
2. Identify public functions or code paths that have no tests
3. Write tests that exercise those paths
4. If they fail, you've found a bug — file an issue
5. If they pass but coverage is missing, file a testing issue
```

## Filing an Issue

### Issue structure

Every issue must be a **complete work order** — someone should be able to fix it without asking questions.

```bash
gh issue create --repo anthropics/claudes-c-compiler \
  --title "[P<LEVEL>] <Short description>" \
  --body "$(cat <<'ISSUE'
## Problem

<What's wrong or what's missing. Be specific.>

## Expected behavior

<What GCC/Clang do, or what the C standard requires.>
<Include the C11 section reference if applicable.>

## Reproduction

```c
// Minimal C program that demonstrates the issue
<code>
```

**CCC output**: <what CCC does>
**GCC output**: <what GCC does>

## Suggested approach

<Which files to modify and what to change.>
<Be specific enough that a contributor can start immediately.>

## Files to modify

- `src/path/to/file.rs` — <what to change in this file>

## Tests to write

<Describe the test cases. Include test code if possible.>

```rust
#[test]
fn test_<name>() {
    // Example test
    assert_eq!(sema_error_count("<C code>"), 1);
}
```

## Acceptance criteria

- [ ] `cargo build --release` passes
- [ ] `cargo test --lib` passes
- [ ] New tests added
- [ ] <Specific criterion for this fix>
ISSUE
)"
```

### Priority levels

| Level | Meaning | Examples |
|-------|---------|---------|
| **P0** | Critical — any C compiler must handle this | Missing error for invalid code, silent miscompilation |
| **P1** | High — important for reliability | CLI silent failures, missing warnings |
| **P2** | Medium — correctness or feature gap | Type system limitation, missing C11 feature |
| **P3** | Low — nice to have | Missing tests, documentation, tooling |

### Title format

```
[P<N>] <Short description>
```

Examples:
- `[P0] Duplicate case labels not detected in switch`
- `[P1] CLI flags -MF, -MT silently ignore missing argument`
- `[P2] -Wshadow not implemented`
- `[P3] Backend codegen has zero unit tests`

## Batch Filing

When you discover multiple related issues (e.g., auditing a single file), file them individually but note the relationship:

```
## Related issues

Part of the <module> audit. See also: #XX, #YY.
```

This helps contributors understand the context and pick related issues.

## Quality Checklist

Before filing, verify:

- [ ] **Reproducible**: You have a minimal C program that demonstrates the issue
- [ ] **Specific**: The issue describes ONE problem, not a vague area
- [ ] **Actionable**: The suggested approach tells someone exactly where to start
- [ ] **Testable**: The acceptance criteria can be verified with `cargo build` + `cargo test`
- [ ] **Not a duplicate**: Check existing open issues first

## Reference Files

- **[TEMPLATES.md](TEMPLATES.md)** — Copy-paste issue body templates by category
