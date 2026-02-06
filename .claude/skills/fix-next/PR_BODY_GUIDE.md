# PR Body Quality Guide

Every PR that goes from draft to ready **must** have a body that a reviewer can learn from.
A thin body wastes the reviewer's time — they have to read the diff, the issue, and reverse-engineer
the "why" themselves. A good body front-loads context so the review is fast and focused.

## Required Structure

Every PR body has exactly four sections:

```
## Problem
## Approach
## Changes
## Test plan
```

### Problem

Explain what was broken and **why it matters**. Not just "X was missing" — explain the impact.

**Good:**
> CCC silently accepts `return 42;` inside a `void` function. C11 §6.8.6.4p1 says
> "A `return` statement with an expression shall not appear in a function whose return
> type is `void`." GCC emits `error: 'return' with a value, in function returning void`.
>
> The existing `-Wreturn-type` check only warns about *missing* returns in non-void
> functions. It doesn't check the inverse. This means `void cleanup(void) { return -1; }`
> compiles silently, and the return value is generated into the IR even though the caller
> never sees it.

**Bad:**
> Added check for return with value in void function.

What makes the good version good:
- C11 spec reference (reviewer can verify correctness)
- What GCC does (reviewer can verify compatibility)
- Why the existing code doesn't catch it (reviewer understands the gap)
- What happens without the fix (reviewer understands the impact)

### Approach

Explain the technical decision and **why this approach over alternatives**.

**Good:**
> The check reuses the existing `switch_cases` stack as a depth indicator — if
> `switch_cases.is_empty()`, there's no enclosing switch. This avoids adding any
> new state to `SemanticAnalyzer`.
>
> Three statement types need the check: `Stmt::Case`, `Stmt::CaseRange` (GCC
> range extension), and `Stmt::Default`. The check runs *before* the duplicate
> case/default checks, so that when we're outside a switch entirely, we don't
> try to access empty stacks.

**Bad:**
> Added check in sema analysis.

What makes the good version good:
- Explains reuse of existing infrastructure (reviewer sees it's minimal)
- Explains ordering constraint (reviewer won't suggest moving the check)
- Mentions edge case coverage (CaseRange, not just Case)

### Changes

List files modified with **specific descriptions** of what changed in each.

**Good:**
> - `src/common/error.rs`: Add `WarningKind::Shadow` — wired into `flag_name()`,
>   `from_flag_name()`, and `all()` (but not `wall_set()`)
> - `src/common/symbol_table.rs`: Add `span: Option<Span>` field to `Symbol`.
>   Add `lookup_outer()` method that skips the innermost scope.
> - `src/frontend/sema/analysis.rs`: Add `span: Some(...)` to all `Symbol`
>   construction sites. Add shadow check before `symbol_table.declare()`.

**Bad:**
> - `src/frontend/sema/analysis.rs`: Added check

### Test plan

One checkbox per **specific behavior verified**. Not generic "tests pass."

**Good:**
> - [x] Duplicate case values → error with note pointing to first
> - [x] Distinct case values → no diagnostic
> - [x] Nested switches with same value → no error (independent scopes)
> - [x] Constant expressions: `case 2+3` and `case 5` detected as duplicate
> - [x] `cargo build --release` — clean build
> - [x] `cargo test --lib` — 500 tests pass (7 new)

**Bad:**
> - [x] `cargo build --release` passes
> - [x] `cargo test --lib` passes
> - [x] New tests added

Always end with `Fixes #<NUMBER>` and the milestone link if applicable:
```
Fixes #20

Part of [MILESTONE] M1: Core Diagnostic Coverage (#40)
```

## Category-Specific Guidance

### Sema diagnostic fixes (new warnings/errors)

- **Problem**: cite the C11 section, show the GCC error message, explain what currently happens
- **Approach**: explain where the check goes (sema vs lowering vs codegen), what state you added to `SemanticAnalyzer`, how nesting/scoping works
- **Changes**: mention new `WarningKind` variants and which methods they're wired into
- **Test plan**: one checkbox per diagnostic scenario (error case, no-error case, edge cases)

### CLI / driver fixes

- **Problem**: show the exact flag, explain the failure mode (silent ignore vs crash vs wrong behavior)
- **Approach**: explain what the fix does (add `else` branch, change return type, replace `unwrap_or`)
- **Changes**: mention the specific flags affected
- **Test plan**: one checkbox per flag tested (both error and valid usage)

### Infrastructure / documentation changes

- **Problem**: explain what was wrong or misleading and the impact on contributors
- **Approach**: explain the design (why this structure, what alternatives you considered)
- **Changes**: list all files touched with what changed in each
- **Test plan**: describe how to verify the change works (may not be unit tests)

## How to Write the Body

When you reach the draft-to-ready step:

1. **Read the diff you just wrote** — understand exactly what you changed
2. **Read the issue body** — that's the context the reviewer will cross-reference
3. **Write the Problem section** from the issue context (don't copy-paste — distill it)
4. **Write the Approach section** from your implementation decisions
5. **Write the Changes section** from the diff (file-by-file)
6. **Write the Test plan** from the tests you wrote (one behavior per checkbox)
