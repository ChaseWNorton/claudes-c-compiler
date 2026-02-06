# Review Checklist

Detailed review criteria organized by issue category.

## Frontend Diagnostic Fixes (P0 issues #20-#24)

These add new compiler errors or warnings in the sema pass.

- [ ] Check is in `src/frontend/sema/analysis.rs` (not in lowering or codegen)
- [ ] Error message matches GCC/Clang phrasing (compare with `gcc -Wall -Werror` output)
- [ ] The span points to the right token (the offending code, not the enclosing statement)
- [ ] If a new `WarningKind` was added, it's wired into all four methods: `flag_name()`, `from_flag_name()`, `wall_set()`, `all()`
- [ ] New state fields on `SemanticAnalyzer` are properly initialized and cleaned up between functions
- [ ] Test uses `sema_error_count()` or `sema_warning_count()` helpers
- [ ] Test covers: the error case, the non-error case, and at least one edge case
- [ ] C11 standard section cited in a comment if applicable

## CLI and Driver Fixes (P1 issues #26-#28)

These fix argument parsing and error reporting in the driver.

- [ ] Missing argument case produces a clear error message
- [ ] Error message includes the flag name (e.g., "argument to '-MF' is missing")
- [ ] The fix doesn't break existing valid usage of the flag
- [ ] Numeric parse errors report the actual invalid value
- [ ] Response file errors include the filename that couldn't be read
- [ ] Test covers: valid usage, missing argument, invalid value (where applicable)

## Frontend Correctness Fixes (P1-P2 issues #29-#32)

These fix type checking, declarations, or other semantic analysis gaps.

- [ ] Type comparison uses semantic equality, not syntactic
- [ ] Fix handles all C type categories (basic types, pointers, arrays, structs, enums, functions)
- [ ] Tentative definition merging follows C11 6.9.2 rules
- [ ] Enum underlying type calculation considers the actual enumerator values
- [ ] Test covers common cases + at least one tricky edge case from the C standard

## Backend Fixes (P2 issue #33)

These fix code generation or calling convention issues.

- [ ] Fix is in the correct backend (x86, i686, arm, riscv)
- [ ] Fix doesn't affect other backends
- [ ] Register usage follows the calling convention specification
- [ ] If the fix changes instruction selection, the new instruction is correct for the operand types
- [ ] Test compiles and runs code that exercises the specific path

## Testing Infrastructure (P3 issues #36-#39)

These add test helpers or test suites.

- [ ] Test helpers are minimal — just enough to test, not over-abstracted
- [ ] Helpers use the existing compilation pipeline (don't reinvent parsing, etc.)
- [ ] Test module is `#[cfg(test)] mod tests` at the bottom of the file
- [ ] Helper functions are documented with a brief comment
- [ ] At least 3 test cases demonstrate the helper works

## Documentation Fixes (P0 issue #25)

- [ ] Old comment is replaced with accurate description (not just deleted)
- [ ] New comment reflects what the code actually does
- [ ] No code changes (documentation-only)

## Universal Checks (apply to all PRs)

- [ ] `cargo build --release` passes with no new warnings
- [ ] `cargo test --lib` passes with zero failures
- [ ] No unrelated changes in the diff
- [ ] No new external dependencies added
- [ ] PR references the correct issue number
