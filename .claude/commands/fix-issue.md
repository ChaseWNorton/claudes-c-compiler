Work on fixing GitHub issue #$ARGUMENTS from the anthropics/claudes-c-compiler repository.

## Pre-flight: Check if already claimed

Before starting, verify no one else is working on this issue:
```bash
gh pr list --repo anthropics/claudes-c-compiler --state open --json body,url --jq '.[] | select(.body | test("Fixes #$ARGUMENTS\\b")) | .url'
```
If an open PR already references `Fixes #$ARGUMENTS`, tell the user it's already claimed and suggest running `/pick-issue` to find another.

## Workflow

1. **Fetch the issue details** (this is your complete work order):
   ```
   gh issue view $ARGUMENTS --repo anthropics/claudes-c-compiler
   ```

2. **Read the issue body** to understand the problem, reproduction steps, suggested approach, and files to modify.

3. **Claim it** — create a branch and draft PR:
   ```
   git switch main && git pull origin main
   git switch -c fix/issue-$ARGUMENTS
   git commit --allow-empty -m "WIP: Fix #$ARGUMENTS: <title>"
   git push -u origin fix/issue-$ARGUMENTS
   gh pr create --repo anthropics/claudes-c-compiler --title "Fix #$ARGUMENTS: <title>" --body "WIP — Fixes #$ARGUMENTS" --draft
   ```
   The draft PR is your claim. Other workers will see it and skip this issue.

4. **Read the files** mentioned in the issue to understand the existing code.

5. **Implement the fix** following the suggested approach in the issue. Follow the patterns described in CLAUDE.md.

6. **Write tests** as described in the issue. Use the existing test helpers:
   - `sema_error_count("C code")` / `sema_warning_count("C code")` for frontend diagnostic tests
   - `compile_to_ir("C code")` for IR-level tests
   - Add tests in `#[cfg(test)] mod tests` at the bottom of the modified file
   If test helpers don't exist yet, create them (see issue #36 for the pattern).

7. **Verify:**
   ```
   cargo build --release && cargo test --lib
   ```

8. **Commit** with a descriptive message:
   ```
   git add <specific-files>
   git commit -m "Fix #$ARGUMENTS: <short description>"
   ```

9. **Push and mark PR ready:**
   ```
   git push
   gh pr ready <PR_NUMBER> --repo anthropics/claudes-c-compiler
   ```

10. **Update the PR body** with the final description:
    ```
    gh pr edit <PR_NUMBER> --repo anthropics/claudes-c-compiler --body "..."
    ```

## PR requirements

- Title: `Fix #$ARGUMENTS: <short description>`
- Body has: Summary, Changes, and Test plan sections
- Body ends with: `Fixes #$ARGUMENTS`
- All existing tests pass + new tests for the fix
- Clean build with no new warnings
