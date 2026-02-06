# Fix Next — Auto-Cycle Issue Fixer

Claim and fix the next available issue from `anthropics/claudes-c-compiler`, then loop.

## Git remote convention

- `origin` = your fork (pushable)
- `upstream` = anthropics/claudes-c-compiler (read-only)

## CRITICAL: AUTO-CYCLE MODE

**When this skill is invoked, you MUST continue fixing issues in a loop until no unclaimed work remains.**

```
LOOP:
  1. Find next unclaimed issue (highest priority first)
  2. If none available → STOP (all done!)
  3. VALIDATE FIRST — read issue, mark [REVIEWING], check if real
     3a. If NOT real → mark [DENIED] with proof, skip, GOTO 1
     3b. If real → mark [WIP], continue
  4. Detect chain tip (highest non-draft [CC] PR, or main)
  5. CLAIM (branch off chain tip + draft PR with [CC] prefix)
  6. Read issue body (= the complete work order)
  7. Read the files mentioned in the issue
  8. Implement the fix
  9. Write tests as described in the issue
  10. Verify: cargo build --release && cargo test --lib
  11. Push
  12. **MARK PR READY** (gh pr ready) — NOT OPTIONAL, a draft is invisible
  13. **MARK ISSUE [COMPLETE]** — update title + comment with PR number
  14. Your [CC] PR is now the chain tip for the next iteration
  15. GOTO 1
```

**DO NOT STOP** after fixing one issue. **DO NOT ASK** the user what to do next. Claim the next issue and continue.

## Chain Protocol

Before creating each branch, detect the PR chain:

```bash
CHAIN_TIP=$(gh pr list --repo anthropics/claudes-c-compiler --state open \
  --json number,title,headRefName,isDraft --limit 100 \
  | jq -r '[.[] | select(.title | test("^\\[CC\\]")) | select(.isDraft | not)] | sort_by(.number) | last')
CHAIN_TIP_NUMBER=$(echo "$CHAIN_TIP" | jq -r '.number // empty')
```

**If chain exists**: branch off the tip (`gh pr checkout $CHAIN_TIP_NUMBER --detach`), add `[CC]` to PR title.
**If no chain**: branch off `main` (standard flow, no `[CC]`).
**After your PR is marked ready**: your PR is the new chain tip. The next loop iteration should detect it.

## Quick Reference

### Find unclaimed work

```bash
# Open issues
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50

# Open PRs — titles contain [Fix #N] for claim detection, [CC] for chain
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title,headRefName,isDraft --limit 100
```

**CRITICAL: Draft PRs are LOCKS, not requests for help.**
An issue is **claimed** if ANY open PR title contains `[Fix #<number>]` — **draft or ready, both count**.
A draft PR means another agent is actively working on that issue. Do NOT touch it, do NOT
try to help, do NOT open a second PR. Skip it immediately.

Subtract claimed from open. Pick highest priority: `[P0]` > `[P1]` > `[P2]` > `[P3]`.
Title codes: `[P<N>]` = priority, `[M<N>]` = milestone membership (informational).
Also skip issues tagged `[DENIED]`, `[WIP]`, `[REVIEWING]`, or `[COMPLETE]`.

### Validate the issue (BEFORE creating any branch or PR)

**CRITICAL: You MUST validate the issue before claiming it.**

1. Read the issue body completely
2. Mark as REVIEWING:
```bash
gh issue edit <NUMBER> --repo anthropics/claudes-c-compiler \
  --title "[REVIEWING]<rest of title without [OPEN]>"
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "Reviewing: investigating whether this issue is valid."
```
3. Check the source code — does the bug actually exist?
4. If NOT real → DENIED (with proof in comment), skip to next issue:
```bash
gh issue edit <NUMBER> --repo anthropics/claudes-c-compiler \
  --title "[DENIED]<rest of title without [REVIEWING]>"
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "Denied — <evidence and reasoning>."
```
5. If real → WIP + proceed to claim:
```bash
gh issue edit <NUMBER> --repo anthropics/claudes-c-compiler \
  --title "[WIP]<rest of title without [REVIEWING]>"
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "Confirmed — <brief explanation>. Proceeding with fix."
```

### Claim an issue (chain-aware, AFTER validation confirms issue is real)

**If chain exists:**
```bash
gh pr checkout $CHAIN_TIP_NUMBER --detach
git switch -c fix/issue-<NUMBER>
git commit --allow-empty -m "WIP: claiming issue #<NUMBER>"
git push -u origin fix/issue-<NUMBER>
gh pr create --repo anthropics/claudes-c-compiler \
  --title "[CC][Fix #<NUMBER>] <description>" \
  --body "WIP — Fixes #<NUMBER>" --draft
```

**If no chain:**
```bash
git switch main && git pull upstream main
git switch -c fix/issue-<NUMBER>
git commit --allow-empty -m "WIP: claiming issue #<NUMBER>"
git push -u origin fix/issue-<NUMBER>
gh pr create --repo anthropics/claudes-c-compiler \
  --title "[Fix #<NUMBER>] <description>" \
  --body "WIP — Fixes #<NUMBER>" --draft
```

### Complete and push

```bash
cargo build --release && cargo test --lib   # Must pass
git add <specific-files>
git commit -m "Fix #<NUMBER>: <short description>"
git push origin fix/issue-<NUMBER>
```

### CRITICAL: Mark PR ready for review

**DO NOT SKIP THIS. A draft PR is invisible to reviewers. The fix is NOT done until you run this:**

```bash
gh pr ready <PR_NUMBER> --repo anthropics/claudes-c-compiler
```

**Then mark the issue COMPLETE:**
```bash
gh issue edit <NUMBER> --repo anthropics/claudes-c-compiler \
  --title "[COMPLETE]<rest of title without [WIP]>"
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "Complete — fix shipped in PR #<PR_NUMBER>. Awaiting merge."
```

Then write the PR body with four sections: Problem, Approach, Changes, Test plan.
See [PR_BODY_GUIDE.md](PR_BODY_GUIDE.md) for the quality standard. End with `Fixes #<NUMBER>`.

## Implementation Rules

1. **Read the issue body completely** — it is the full work order.
2. **Read the source files** before writing any code. See [CODEBASE_PATTERNS.md](CODEBASE_PATTERNS.md) for per-category guidance.
3. **Follow CLAUDE.md conventions** — tests in `#[cfg(test)] mod tests`, GCC-format error messages, no external deps.
4. **Write tests** as described in the issue. Check if helpers exist before creating them.
5. **Verify**: `cargo build --release && cargo test --lib` — both must pass with zero failures.
6. **One commit per issue** — message format: `Fix #<NUMBER>: <short description>`

## Reference Files

- **[COORDINATION.md](COORDINATION.md)** — Detailed claim/release protocol, race conditions, stale claim handling
- **[CODEBASE_PATTERNS.md](CODEBASE_PATTERNS.md)** — How to fix each issue category (diagnostics, CLI, backend, tests)
- **[PR_BODY_GUIDE.md](PR_BODY_GUIDE.md)** — PR body quality standard: 4-section structure, good/bad examples, category guidance
- **[TROUBLESHOOTING.md](TROUBLESHOOTING.md)** — Build failures, test failures, claim conflicts, recovery

## Error Recovery

- **Build fails**: Fix the error. Amend is OK on your WIP branch.
- **Tests fail**: Fix the implementation. Don't mark PR ready until tests pass.
- **Already claimed**: Skip it, next unclaimed issue.
- **Issue unclear**: Re-read the reproduction code and suggested approach in the issue body.
- **Stuck**: See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for detailed recovery steps.

## Behavior

**WRONG:**
- Fixing one issue then asking "Should I continue?"
- Stopping after each fix to report progress
- Waiting for user confirmation

**CORRECT:**
- Fix → PR → next → Fix → PR → next → ...
- Only stop when no unclaimed issues remain
