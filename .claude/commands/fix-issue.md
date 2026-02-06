Work on fixing GitHub issue #$ARGUMENTS from the anthropics/claudes-c-compiler repository.

## Git remote convention

- `origin` = your fork (pushable)
- `upstream` = anthropics/claudes-c-compiler (read-only)

## Pre-flight: Check if already claimed

Check PR titles for `[Fix #$ARGUMENTS]`:
```bash
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50
```
If any PR title contains `[Fix #$ARGUMENTS]`, it's already claimed. Tell the user and suggest picking another issue.

## Phase 1: Claim (before writing any code)

1. **Fetch the issue title** for the PR:
   ```bash
   gh issue view $ARGUMENTS --repo anthropics/claudes-c-compiler --json title --jq '.title'
   ```

2. **Create branch and draft PR**:
   ```bash
   git switch main && git pull upstream main
   git switch -c fix/issue-$ARGUMENTS
   git commit --allow-empty -m "WIP: claiming issue #$ARGUMENTS"
   git push -u origin fix/issue-$ARGUMENTS
   gh pr create --repo anthropics/claudes-c-compiler \
     --title "[Fix #$ARGUMENTS] <description from issue title without priority codes>" \
     --body "WIP — Fixes #$ARGUMENTS" --draft
   ```
   The draft PR is your claim. The `[Fix #$ARGUMENTS]` in the title lets other workers detect it from titles alone.

## Phase 2: Fix

3. **Read the issue body** (this is the complete work order):
   ```bash
   gh issue view $ARGUMENTS --repo anthropics/claudes-c-compiler
   ```

4. **Read the files** mentioned in the issue to understand the existing code.

5. **Implement the fix** following the suggested approach in the issue. Follow CLAUDE.md patterns.

6. **Write tests** as described in the issue:
   - `sema_error_count("C code")` / `sema_warning_count("C code")` for frontend diagnostic tests
   - `compile_to_ir("C code")` for IR-level tests
   - Add tests in `#[cfg(test)] mod tests` at the bottom of the modified file

7. **Verify:**
   ```bash
   cargo build --release && cargo test --lib
   ```

## Phase 3: Ship

8. **Commit and push:**
   ```bash
   git add <specific-files>
   git commit -m "Fix #$ARGUMENTS: <short description>"
   git push origin fix/issue-$ARGUMENTS
   ```

9. **Mark PR ready and update body:**
   ```bash
   gh pr ready <PR_NUMBER> --repo anthropics/claudes-c-compiler
   gh pr edit <PR_NUMBER> --repo anthropics/claudes-c-compiler --body "$(cat <<'EOF'
   ## Summary
   <what was wrong and why>

   ## Changes
   <what you changed>

   ## Test plan
   - [x] `cargo build --release` passes
   - [x] `cargo test --lib` passes
   - [x] New tests added

   Fixes #$ARGUMENTS
   EOF
   )"
   ```

## PR requirements

- Title: `[Fix #$ARGUMENTS] <description>`
- Body ends with: `Fixes #$ARGUMENTS`
- All existing tests pass + new tests for the fix
- Clean build with no new warnings
