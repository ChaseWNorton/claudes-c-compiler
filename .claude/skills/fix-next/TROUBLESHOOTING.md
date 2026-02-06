# Troubleshooting

Common problems during the fix-next cycle and how to recover.

## Git remote convention

- `origin` = your fork (pushable)
- `upstream` = anthropics/claudes-c-compiler (read-only)

## Contents

- Build failures
- Test failures
- Claim conflicts
- Git issues
- PR issues
- Cycle interruption

## Build Failures

### Syntax error in your changes

**Symptom**: `cargo build --release` fails with a compiler error pointing to your code.

**Fix**: Read the error message carefully. Rust compiler errors are precise — the line number and explanation tell you exactly what's wrong. Fix and rebuild.

### Borrow checker error

**Symptom**: `cannot borrow X as mutable because it is also borrowed as immutable`

**Fix**: This is common when adding new checks to the sema pass. The `diagnostics` field uses `RefCell` for interior mutability. Make sure you:
1. Don't hold a borrow across a diagnostic emission
2. Use `self.diagnostics.borrow_mut()` for the shortest possible scope

```rust
// WRONG: holding borrow across method call
let diag = self.diagnostics.borrow_mut();
self.analyze_something(); // might also borrow diagnostics
diag.error(msg, span);

// RIGHT: borrow only for the emission
self.analyze_something();
self.diagnostics.borrow_mut().error(msg, span);
```

### Type mismatch

**Symptom**: `expected X, found Y` type errors.

**Fix**: Read the type definitions in `src/common/types.rs`. CCC has its own type system (`CType`, `IrType`) — make sure you're using the right one for the context (frontend uses `CType`, IR uses `IrType`).

## Test Failures

### Existing test fails after your change

**Symptom**: `cargo test --lib` shows a failure in a test you didn't write.

**Fix**: Your change may have introduced a new diagnostic that an existing test wasn't expecting. Check what the test does:
1. Run the specific failing test: `cargo test --lib -- test_name`
2. Read the test code to understand what it expects
3. If your change correctly added a new diagnostic, update the test's expected count
4. If your change broke something, revert and rethink

### Your new test fails

**Symptom**: The test you wrote produces unexpected results.

**Fix**:
1. Check the C code in your test — is it valid C? Does it actually trigger the condition you're testing?
2. Check the expected count — off-by-one is common when multiple diagnostics fire
3. Add a simpler test case to isolate the issue
4. Use `cargo test --lib -- test_name --nocapture` to see println output

### Test helper doesn't exist yet

**Symptom**: `sema_error_count` or `sema_warning_count` is not defined.

**Fix**: Check if issue #36 (test infrastructure) has been completed. If not, you need to create the helpers yourself. See [CODEBASE_PATTERNS.md](CODEBASE_PATTERNS.md) Category 5.

## Claim Conflicts

### Someone else claimed the issue while you were working

**Symptom**: When you push, you discover another PR exists for the same issue.

**Fix**:
1. Check who was first: `gh pr list --repo anthropics/claudes-c-compiler --state open --json number,createdAt,title`
2. If you were second, close your PR: `gh pr close <YOUR_PR> --repo anthropics/claudes-c-compiler`
3. Move on to the next unclaimed issue

### Your draft PR was closed by someone else

**Symptom**: Your PR was closed as "stale" while you were still working.

**Fix**: Reopen it or create a new one:
```bash
# Option A: reopen
gh pr reopen <PR_NUMBER> --repo anthropics/claudes-c-compiler

# Option B: create new PR from your existing branch
git push origin fix/issue-<NUMBER>
gh pr create --repo anthropics/claudes-c-compiler \
  --title "[Fix #<NUMBER>] <description>" \
  --body "Fixes #<NUMBER>" --draft
```

## Git Issues

### Branch already exists

**Symptom**: `git switch -c fix/issue-20` fails because the branch exists.

**Fix**: You may have started this issue before. Check if there's work on it:
```bash
git log fix/issue-20 --oneline -5
```
If it has your previous work, switch to it and continue. If it's stale, delete and recreate:
```bash
git branch -D fix/issue-20
git switch -c fix/issue-20
```

### Merge conflict with main

**Symptom**: `git pull upstream main` fails with merge conflicts.

**Fix**: You shouldn't need to merge main into your fix branch — each fix is independent. If someone else's merged PR conflicts with yours:
1. Start fresh from main: create a new branch
2. Re-apply your changes on top of the latest main
3. Force-push to your fix branch on origin (it's your WIP branch, this is OK)

### Push rejected

**Symptom**: `git push origin fix/issue-<NUMBER>` is rejected because the remote has changes you don't have.

**Fix**: This usually means you force-pushed or amended earlier. Pull and resolve:
```bash
git pull --rebase origin fix/issue-<NUMBER>
git push origin fix/issue-<NUMBER>
```

## PR Issues

### PR creation fails

**Symptom**: `gh pr create` returns an error.

**Common causes**:
- Branch doesn't exist on your fork yet: `git push -u origin fix/issue-<NUMBER>` first
- PR already exists for this branch: check with `gh pr list --head fix/issue-<NUMBER>`
- Authentication: make sure `gh auth status` shows you're logged in
- Pushing to wrong remote: make sure `origin` is your fork, not the upstream repo

### PR body formatting is broken

**Symptom**: The PR body doesn't render correctly on GitHub.

**Fix**: Use HEREDOC syntax carefully. Make sure `EOF` markers are not indented:
```bash
gh pr edit <PR_NUMBER> --body "$(cat <<'EOF'
## Summary
Content here.

Fixes #<NUMBER>
EOF
)"
```

## Cycle Interruption

### Claude Code session ended mid-fix

**Recovery**:
1. Check which branch you were on: `git branch --show-current`
2. Check the git log: `git log --oneline -5`
3. If there's a draft PR, continue where you left off
4. If build/tests pass, finalize the PR
5. If not, fix and continue

### Want to skip a hard issue

If an issue is too complex or unclear, skip it and move to the next one. Do NOT close the draft PR if you haven't started — just don't create one. If you already have a draft PR, close it to release the claim:

```bash
gh pr close <PR_NUMBER> --repo anthropics/claudes-c-compiler \
  --comment "Skipping — issue requires more investigation."
```

Then continue the cycle with the next unclaimed issue.
