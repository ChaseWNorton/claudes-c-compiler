# Fix Next — Auto-Cycle Issue Fixer

Claim and fix the next available issue from `anthropics/claudes-c-compiler`, then loop.

## CRITICAL: AUTO-CYCLE MODE

**When this skill is invoked, you MUST continue fixing issues in a loop until no unclaimed work remains.**

```
LOOP:
  1. Find next unclaimed issue (highest priority first)
  2. If none available → STOP (all done!)
  3. Claim it (create branch + draft PR)
  4. Read issue body (= the complete work order)
  5. Read the files mentioned in the issue
  6. Implement the fix
  7. Write tests as described in the issue
  8. Verify: cargo build --release && cargo test --lib
  9. Push, mark PR ready for review
  10. GOTO 1
```

**DO NOT STOP** after fixing one issue. **DO NOT ASK** the user what to do next. Claim the next issue and continue.

## Quick Reference

### Find unclaimed work

```bash
# Open issues
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50

# Already claimed (issue numbers referenced by open PRs)
gh pr list --repo anthropics/claudes-c-compiler --state open --json body --jq '.[].body' | grep -oP 'Fixes #\K[0-9]+' | sort -u
```

Subtract claimed from open. Pick highest priority: `[P0]` > `[P1]` > `[P2]` > `[P3]`.

### Claim an issue

```bash
git switch main && git pull origin main
git switch -c fix/issue-<NUMBER>
git commit --allow-empty -m "WIP: Fix #<NUMBER>: <title>"
git push -u origin fix/issue-<NUMBER>
gh pr create --repo anthropics/claudes-c-compiler \
  --title "Fix #<NUMBER>: <title>" \
  --body "WIP — Fixes #<NUMBER>" --draft
```

### Complete and finalize

```bash
cargo build --release && cargo test --lib   # Must pass
git add <specific-files>
git commit -m "Fix #<NUMBER>: <short description>"
git push
gh pr ready <PR_NUMBER> --repo anthropics/claudes-c-compiler
```

Update PR body with Summary, Changes, Test plan. End with `Fixes #<NUMBER>`.

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
