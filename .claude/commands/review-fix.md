Review pull request #$ARGUMENTS on the CCC compiler project.

## Git remote convention

- `origin` = your fork (pushable)
- `upstream` = anthropics/claudes-c-compiler (read-only)

## Steps

1. **Fetch the PR and its linked issue**:
   ```bash
   gh pr view $ARGUMENTS --repo anthropics/claudes-c-compiler --json title,body,files
   ```
   Extract the issue number from `[Fix #N]` in the PR title or `Fixes #N` in the PR body, then:
   ```bash
   gh issue view <ISSUE_NUMBER> --repo anthropics/claudes-c-compiler
   ```

2. **Read the diff**:
   ```bash
   gh pr diff $ARGUMENTS --repo anthropics/claudes-c-compiler
   ```

3. **Review against the issue's acceptance criteria** — check that:
   - The fix addresses the problem in the issue
   - Tests are present and cover the fix
   - Code follows existing patterns
   - PR has Summary, Changes, Test plan sections
   - Build and tests pass

4. **Checkout and verify locally** (if needed):
   ```bash
   gh pr checkout $ARGUMENTS
   cargo build --release && cargo test --lib
   ```

5. **Submit your review** (adapts to your access level):
   - **Maintainers**: formal review — approve or request changes
   - **Contributors**: post findings as a PR comment

See the review-fix skill for the full checklist by issue category.
