# Codebase Patterns

How to fix different categories of issues in CCC. Read this before implementing any fix — it tells you which files to read, what patterns to follow, and what test helpers exist.

## Contents

- Category 1: Frontend diagnostics (errors/warnings)
- Category 2: CLI and driver issues
- Category 3: Frontend correctness (type system, sema)
- Category 4: Backend correctness (codegen)
- Category 5: Testing infrastructure
- Category 6: Documentation

## Category 1: Frontend Diagnostics

**Issues**: Missing compiler warnings or errors (duplicate case labels, void return, etc.)

**Files to read first**:
- `src/frontend/sema/analysis.rs` — the semantic analysis pass. This is where all diagnostics live.
- `src/common/error.rs` — the `DiagnosticEngine`, `WarningKind` enum, `WarningConfig`.

**Pattern for adding a new error**:
1. Find the relevant `analyze_*` method in `analysis.rs` (e.g., `analyze_stmt` for statement-level checks)
2. Add the check at the appropriate point in the analysis
3. Emit the error: `self.diagnostics.borrow_mut().error(msg, span)`
4. Error messages should match GCC format: `"error: <description>"`

**Pattern for adding a new warning**:
1. Add a new variant to `WarningKind` in `src/common/error.rs`
2. Wire it into these methods on `WarningKind`:
   - `flag_name()` — returns the `-W` flag string (e.g., `"shadow"`)
   - `from_flag_name()` — parses the string back to the variant
   - `wall_set()` — whether `-Wall` enables this warning
   - `all()` — list of all warning kinds
3. Add the check in `analysis.rs`
4. Emit: `self.diagnostics.borrow_mut().warning_with_kind(msg, span, WarningKind::YourKind)`

**Test helpers**:
```rust
// Count errors produced by semantic analysis
let count = sema_error_count("int main() { return \"hello\"; }");
assert_eq!(count, 1);

// Count warnings produced
let count = sema_warning_count("int f() { int x; }");
assert_eq!(count, 1);
```

If these helpers don't exist yet, create them at the bottom of `analysis.rs`:
```rust
#[cfg(test)]
fn sema_error_count(code: &str) -> usize {
    // Parse and run sema, return error count from diagnostics
}
```

Check issue #36 for the full test infrastructure pattern.

**Common pitfalls**:
- The sema pass runs BEFORE IR lowering. Add diagnostics in sema, not in lowering.
- Some checks need state tracked across the function (e.g., duplicate case labels need a `HashSet`). Add fields to `SemanticAnalyzer` for this.
- The `span` parameter matters — it determines where the error points in the source. Use the span of the offending token, not the enclosing statement.

## Category 2: CLI and Driver Issues

**Issues**: Silent failures in argument parsing, missing error messages for bad flags.

**Files to read first**:
- `src/driver/cli.rs` — all CLI parsing logic. The `parse_args()` function.

**Pattern for fixing missing-argument errors**:

Many flags consume the next argument (e.g., `-MF <file>`). The current code sometimes silently ignores a missing argument. Fix by checking the iterator:

```rust
// WRONG (current):
"-MF" => {
    // Silently fails if no next argument
    if let Some(arg) = args.next() {
        options.dep_file = Some(arg);
    }
}

// RIGHT (fixed):
"-MF" => {
    match args.next() {
        Some(arg) => options.dep_file = Some(arg),
        None => {
            diagnostics.error("argument to '-MF' is missing", Span::default());
            return Err(());
        }
    }
}
```

**Pattern for fixing numeric parse errors**:

The current code uses `.unwrap_or(0)` for numeric flags. Fix by reporting the parse error:

```rust
// WRONG: silently defaults to 0
let value = s.parse::<u32>().unwrap_or(0);

// RIGHT: report the error
let value = match s.parse::<u32>() {
    Ok(v) => v,
    Err(_) => {
        diagnostics.error(&format!("invalid integer argument: '{}'", s), Span::default());
        return Err(());
    }
};
```

**Test pattern**: CLI tests typically test the full compilation pipeline. Check if there are existing CLI-specific tests; if not, test by verifying the error is emitted.

## Category 3: Frontend Correctness

**Issues**: Type checking gaps, missing merging, enum types.

**Files to read first**:
- `src/frontend/sema/analysis.rs` — type checking logic
- `src/common/types.rs` — `CType` enum, type representation, `EnumType`
- `src/frontend/parser/` — AST definitions

**Pattern for type compatibility checks**:

The sema pass has access to both the declared type and the expression type. Compare them:

```rust
// In analyze_return or similar:
if let Some(expected) = &self.current_function_return_type {
    let actual = self.type_of_expr(expr);
    if !types_compatible(expected, &actual) {
        self.diagnostics.borrow_mut().error(
            &format!("returning '{}' from a function with return type '{}'", actual, expected),
            expr.span,
        );
    }
}
```

**Pattern for tentative definitions** (C11 6.9.2):

Tentative definitions require tracking at file scope. The sema pass needs to remember previous declarations of the same symbol and merge them if both are tentative.

**Common pitfalls**:
- `CType` comparison: use the semantic equality, not struct equality. Two `int` types should be equal even if they come from different parse locations.
- The parser already handles much of the syntax — sema's job is to validate semantics.

## Category 4: Backend Correctness

**Issues**: Wrong instructions, calling convention bugs.

**Files to read first**:
- `src/backend/x86/codegen.rs` — x86-64 code generation
- `src/backend/i686/codegen.rs` — 32-bit x86 code generation
- `src/backend/arm/codegen.rs` — AArch64 code generation

**Pattern**: Backend bugs usually require reading the codegen for the specific instruction or calling convention in question. The issue body will specify which backend and what's wrong.

**Test pattern**: Backend tests typically use `compile_to_ir()` or full-pipeline tests. For codegen-specific tests, you may need to compile code and inspect the generated assembly.

**Common pitfalls**:
- Each backend has its own register allocator and calling convention implementation
- Changes to one backend should not affect others
- Test on the specific architecture mentioned in the issue

## Category 5: Testing Infrastructure

**Issues**: Missing test helpers, missing test suites.

**Files to modify**: The test module at the bottom of the file being tested.

**Pattern for adding test helpers**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: compile C code through the frontend and return error count
    fn sema_error_count(code: &str) -> usize {
        let diagnostics = DiagnosticEngine::new();
        // ... tokenize, parse, analyze ...
        diagnostics.error_count()
    }

    #[test]
    fn test_duplicate_case_labels() {
        assert_eq!(sema_error_count("int f() { switch(0) { case 1: case 1: ; } }"), 1);
    }
}
```

**Key principle**: Test helpers should be minimal — just enough to compile the code through the relevant pipeline stage and check the output. Don't over-abstract.

**Common pitfalls**:
- Tests go in `#[cfg(test)] mod tests` at the bottom of the file, not in a separate file
- Import `super::*` to get access to the module's types
- Use `cargo test --lib -- test_name` to run a specific test

## Category 6: Documentation

**Issues**: Outdated comments, incorrect claims in source.

**Pattern**: Read the code, understand what it actually does, update the comment to match reality. These are typically the simplest fixes.

**Common pitfalls**:
- Don't just delete the comment — replace it with an accurate one
- If the comment was a TODO that's been completed, say what's now implemented
