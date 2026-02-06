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

## Coordination Protocol

GitHub Issues + PRs are the shared state. No local state file needed.

### How claiming works

- **Available** = open issue with NO open PR whose body contains `Fixes #N`
- **Claimed** = an open PR exists with `Fixes #N` in the body
- **Done** = PR merged → issue auto-closes

### Finding unclaimed work

```bash
# Step 1: Get all open issues (sorted by priority prefix)
ISSUES=$(gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50)

# Step 2: Get all issue numbers referenced by open PRs
CLAIMED=$(gh pr list --repo anthropics/claudes-c-compiler --state open --json body --jq '.[].body' | grep -oP 'Fixes #\K[0-9]+' | sort -u)

# Step 3: Find the first unclaimed issue (P0 first, then P1, P2, P3)
# Parse ISSUES, filter out anything in CLAIMED, pick the highest priority one
```

Priority order: `[P0]` > `[P1]` > `[P2]` > `[P3]` > unprefixed

### Claiming an issue

The claim is creating a **branch and a draft PR**. This tells other workers "I'm on it."

```bash
# Create branch
git switch -c fix/issue-<NUMBER>

# Create an empty commit so we can push
git commit --allow-empty -m "WIP: Fix #<NUMBER>: <title>"

# Push and create draft PR (= the claim)
git push -u origin fix/issue-<NUMBER>
gh pr create --repo anthropics/claudes-c-compiler \
  --title "Fix #<NUMBER>: <title>" \
  --body "$(cat <<'EOF'
## Summary
Work in progress — implementing fix.

## Changes
(will be updated when complete)

## Test plan
(will be updated when complete)

Fixes #<NUMBER>
EOF
)" --draft
```

### Completing the work

After implementing the fix and verifying tests pass:

```bash
# Stage and commit the actual fix
git add <specific-files>
git commit -m "Fix #<NUMBER>: <short description>"

# Push (updates the draft PR)
git push

# Mark PR as ready for review (= mark done)
gh pr ready <PR_NUMBER> --repo anthropics/claudes-c-compiler

# Update PR body with real Summary, Changes, Test plan
gh pr edit <PR_NUMBER> --repo anthropics/claudes-c-compiler --body "$(cat <<'EOF'
## Summary
<what was wrong and why>

## Changes
<what you changed, file by file>

## Test plan
- [x] `cargo build --release` — clean build
- [x] `cargo test --lib` — all tests pass
- [x] New tests added for the fix

Fixes #<NUMBER>
EOF
)"
```

### Switching to next issue

After completing one issue, switch back to main before starting the next:

```bash
git switch main
git pull origin main
```

Then find the next unclaimed issue and repeat.

## Implementation Rules

1. **Read the issue body completely** — it contains the full work order: problem, reproduction, suggested approach, files to modify, tests to write, acceptance criteria.

2. **Read the files mentioned** before writing any code. Understand existing patterns.

3. **Follow CLAUDE.md conventions**:
   - Tests in `#[cfg(test)] mod tests` at the bottom of the modified file
   - Match GCC/Clang error message format
   - No external dependencies
   - Cite C11 standard sections where relevant

4. **Write tests** as described in the issue. If the issue says to add `sema_error_count` / `sema_warning_count` test helpers, check if they already exist first (a previous issue may have added them).

5. **Verify before marking done**:
   ```bash
   cargo build --release && cargo test --lib
   ```
   Both must pass with zero failures.

6. **One commit per issue** — keep it clean. The commit message should be:
   ```
   Fix #<NUMBER>: <short description>
   ```

## Error Recovery

- **Build fails**: Fix the error, amend is OK since it's your WIP branch.
- **Tests fail**: Fix the test or the implementation. Don't mark PR ready until tests pass.
- **Issue is unclear**: Read the reproduction code and suggested approach more carefully. The issues are self-contained work orders — all context is in the body.
- **Already claimed**: Skip it, move to the next unclaimed issue.

## Example Auto-Cycle Session

```
1. Find unclaimed → #20 [P0] Duplicate case labels
2. Create branch fix/issue-20, draft PR → claimed
3. Read issue body, read analysis.rs
4. Add switch_cases tracking to SemanticAnalyzer
5. Write tests, cargo build && cargo test → PASS
6. Push, mark PR ready
7. Switch to main, pull
8. Find unclaimed → #21 [P0] Duplicate default labels    ← IMMEDIATELY continue
9. Create branch fix/issue-21, draft PR → claimed
10. Read issue body, implement fix
11. Write tests, verify → PASS
12. Push, mark PR ready
13. Switch to main, pull
14. Find unclaimed → #22 [P0] Void return                ← IMMEDIATELY continue
... repeat until ...
N. Find unclaimed → none available                        ← STOP only here
```

**WRONG behavior:**
- Fixing one issue then asking "Should I continue?"
- Stopping after each fix to report progress
- Waiting for user confirmation

**CORRECT behavior:**
- Fix → PR → next → Fix → PR → next → ...
- Only stop when no unclaimed issues remain
