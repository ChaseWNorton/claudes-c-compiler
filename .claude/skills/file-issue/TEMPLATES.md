# Issue Templates

Copy-paste templates for filing different categories of issues.

## Diagnostic Issue (missing error/warning)

```markdown
## Problem

CCC does not diagnose <condition>. This is a <error|warning> that GCC and Clang both emit.

## Expected behavior

Per C11 <section>, <what the standard says>.

GCC emits:
```
<file>:<line>:<col>: <error|warning>: <message>
```

## Reproduction

```c
<minimal C code that triggers the condition>
```

**CCC**: compiles without diagnostics (WRONG)
**GCC**: `<error|warning>: <message>`

## Suggested approach

In `src/frontend/sema/analysis.rs`, in the `<method>` method:

1. <what to track or check>
2. <how to emit the diagnostic>
3. <any state management needed>

## Files to modify

- `src/frontend/sema/analysis.rs` — Add the diagnostic check
- `src/common/error.rs` — Add `WarningKind` variant (if warning, not error)

## Tests to write

```rust
#[test]
fn test_<name>() {
    assert_eq!(sema_error_count("<trigger code>"), 1);
    assert_eq!(sema_error_count("<non-trigger code>"), 0);
}
```

## Acceptance criteria

- [ ] `cargo build --release` passes
- [ ] `cargo test --lib` passes
- [ ] New diagnostic matches GCC format
- [ ] Test covers trigger and non-trigger cases
```

## CLI Validation Issue

```markdown
## Problem

The CLI flag `<flag>` silently <ignores missing argument | accepts invalid value | fails>.

## Expected behavior

GCC reports: `<error message>`

## Reproduction

```bash
# This should produce an error but doesn't
./target/release/ccc <flag> <invalid usage>
```

## Suggested approach

In `src/driver/cli.rs`, in the match arm for `"<flag>"`:

1. Check for <missing argument | invalid value>
2. Emit an error via `diagnostics.error("<message>", Span::default())`
3. Return `Err(())`

## Files to modify

- `src/driver/cli.rs` — Fix the flag handling

## Tests to write

Test that valid usage still works and invalid usage produces an error.

## Acceptance criteria

- [ ] `cargo build --release` passes
- [ ] `cargo test --lib` passes
- [ ] Invalid usage produces a clear error message
- [ ] Valid usage is not affected
```

## Backend Correctness Issue

```markdown
## Problem

<Architecture> backend generates incorrect <instruction | register usage | calling convention> for <condition>.

## Expected behavior

According to the <ABI / ISA spec>, <what should happen>.

## Reproduction

```c
<minimal C code that triggers incorrect codegen>
```

Compile with `ccc-<arch>` and inspect the assembly (`-S` flag) or observe runtime failure.

## Suggested approach

In `src/backend/<arch>/codegen.rs`, in the `<method>` method:

1. <what's wrong with the current codegen>
2. <what it should generate instead>

## Files to modify

- `src/backend/<arch>/codegen.rs` — Fix the code generation

## Tests to write

Compile the reproduction case and verify correct behavior.

## Acceptance criteria

- [ ] `cargo build --release` passes
- [ ] `cargo test --lib` passes
- [ ] Generated assembly uses the correct instruction/register
```

## Testing Infrastructure Issue

```markdown
## Problem

<Module> has zero unit tests. This means regressions can be introduced silently.

## Expected behavior

Core functionality should have test coverage with helpers that make it easy to write more tests.

## Suggested approach

At the bottom of `src/<path>.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: <description>
    fn <helper_name>(<params>) -> <return> {
        // <implementation>
    }

    #[test]
    fn test_<basic_case>() {
        // <test>
    }
}
```

## Files to modify

- `src/<path>.rs` — Add test module with helpers and initial tests

## Tests to write

- At least 3 tests demonstrating the helpers work
- Cover: basic case, edge case, error case

## Acceptance criteria

- [ ] `cargo build --release` passes
- [ ] `cargo test --lib` passes
- [ ] Test helpers are documented
- [ ] At least 3 tests present
```

## Documentation Issue

```markdown
## Problem

<Comment | README | doc> says <what it says>, but the code actually <what it does>.

## Reproduction

In `src/<path>.rs` at line <N>:
```rust
// <the inaccurate comment>
```

But the code below it <describes what it actually does>.

## Suggested approach

Replace the comment with an accurate description of what the code does.

## Files to modify

- `src/<path>.rs` — Update the comment at line <N>

## Acceptance criteria

- [ ] Comment accurately describes the code
- [ ] No code changes (documentation only)
```
