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

Use the structured format from the `file-issue` skill templates:

```bash
gh issue create --repo anthropics/claudes-c-compiler \
  --title "[P<N>] <Short description>" \
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

Note the last line: `Part of [MILESTONE] M<N>: <milestone name> (#<MILESTONE_NUMBER>)` — this links the issue to its milestone.

### 6. Update the milestone issue

After filing all issues, update the milestone's checklist:

```bash
gh issue edit <MILESTONE_NUMBER> --repo anthropics/claudes-c-compiler --body "$(cat <<'EOF'
## Goal
<preserved from original>

## Scope
<preserved from original>

## Issues
- [ ] #<NEW_1> <title>
- [ ] #<NEW_2> <title>
- [ ] #<NEW_3> <title>
...

## Success criteria
<preserved from original>

## Dependencies
<preserved from original>
EOF
)"
```

GitHub will auto-check boxes as issues close.

### 7. Report

```
DECOMPOSED: [MILESTONE] M<N>: <name>

Filed X new issues:
  #XX [P0] <title>
  #XX [P0] <title>
  #XX [P1] <title>
  ...

Milestone issue #<MILESTONE_NUMBER> updated with checklist.

Next: Contributors can run /fix-next to start working on these issues.
```

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
