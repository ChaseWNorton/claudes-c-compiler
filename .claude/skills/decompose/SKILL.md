# Decompose — Break Milestones into Issues

Take a `[MILESTONE]` issue and break it into concrete, actionable GitHub issues that
workers can claim and fix.

## When to Use

Use this skill when:
- A new milestone has been created by `/roadmap` and needs to be broken into work
- A milestone's issue list is empty or incomplete
- You want to add more issues to an existing milestone

## Workflow

### 1. Read the milestone

```bash
gh issue view <MILESTONE_NUMBER> --repo anthropics/claudes-c-compiler
```

Extract:
- **Goal** — what the milestone achieves
- **Scope** — what's included/excluded
- **Success criteria** — how to know it's done
- **Existing issues** — what's already been filed (avoid duplicates)

### 2. Analyze the relevant codebase

Based on the milestone's scope, read the relevant source files:

- For diagnostic milestones → `src/frontend/sema/analysis.rs`, `src/common/error.rs`
- For CLI milestones → `src/driver/cli.rs`
- For backend milestones → `src/backend/*/codegen.rs`
- For testing milestones → test modules across the codebase
- For type system milestones → `src/common/types.rs`, `src/frontend/sema/`

### 3. Identify individual work items

Break the milestone's goal into the smallest independent units of work. Each should be:
- **One issue** = one PR = one fix
- **Self-contained** — can be worked on without completing other issues in the milestone
- **Testable** — has clear acceptance criteria
- **Sized for one session** — a single Claude Code session should be able to complete it

### 4. Check for duplicates

```bash
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50
```

Don't create issues that already exist. If an existing issue partially overlaps, reference it.

### 5. File each issue

Title format: `[P<N>][M<N>] <Short description>` — priority code + milestone code.

```bash
gh issue create --repo anthropics/claudes-c-compiler \
  --title "[P<N>][M<N>] <Short description>" \
  --body "$(cat <<'ISSUE'
## Problem
<specific problem>

## Expected behavior
<what should happen, with C11 reference if applicable>

## Reproduction
```c
<minimal C code>
```

## Suggested approach
<specific files and changes>

## Files to modify
- `src/path/file.rs` — <what to change>

## Tests to write
<specific test cases>

## Acceptance criteria
- [ ] `cargo build --release` passes
- [ ] `cargo test --lib` passes
- [ ] New tests added
- [ ] <specific criterion>

Part of [MILESTONE] M<N>: <milestone name> (#<MILESTONE_NUMBER>)
ISSUE
)"
```

The `[M<N>]` in the title is the machine-readable link. The `Part of [MILESTONE]...` line
in the body is the human-readable link. Both point to the same milestone.

### 6. Report

```
DECOMPOSED: [MILESTONE] M<N>: <name> (#<MILESTONE_NUMBER>)

Filed X new issues:
  #XX [P0] <title>
  #XX [P0] <title>
  #XX [P1] <title>
  ...

Each issue links back to the milestone via:
  "Part of [MILESTONE] M<N>: <name> (#<MILESTONE_NUMBER>)"

Progress is computed dynamically — no need to edit the milestone issue.
```

**Do NOT edit the milestone issue body.** The milestone is write-once. Sub-issues link
back to it. Progress is computed from titles — no need to read bodies:

```bash
# Find all issues belonging to milestone M1 (open and closed)
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 100
gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title --limit 100
# Filter titles containing [M1]
```

This works regardless of who created the milestone — no edit permissions needed.

## Decomposition Principles

1. **One issue = one concept** — "fix duplicate case labels" not "fix all switch statement issues"
2. **Priority matches the milestone** — P0 milestone → mostly P0 issues
3. **Order doesn't matter** — issues should be independent so workers can claim any of them
4. **Include the "how"** — every issue has a suggested approach with specific files
5. **Include tests** — every issue describes what tests to write
6. **Link to milestone** — every issue body references its parent milestone

## Handling Large Milestones

If a milestone would produce more than 15 issues:
1. Split the milestone into sub-milestones
2. File the sub-milestones as `[MILESTONE]` issues
3. Decompose each sub-milestone separately
4. Link sub-milestones to the parent milestone

Example:
```
[MILESTONE] M3: Full Warning Coverage
  └── [MILESTONE] M3a: -Wall Warnings
  └── [MILESTONE] M3b: -Wextra Warnings
  └── [MILESTONE] M3c: -Wpedantic Warnings
```
