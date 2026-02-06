# Fix Next Issue (Auto-Cycle)

Claim and fix the next available issue, then loop until no unclaimed work remains.
**Continue in a loop until all issues are claimed or fixed.**

## Git remote convention

- `origin` = your fork (pushable)
- `upstream` = anthropics/claudes-c-compiler (read-only)

## Instructions

**LOOP until no unclaimed work available:**

1. **Find unclaimed issues** — titles only:
   ```bash
   # Open issues
   gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50

   # Claimed issue numbers (parse [Fix #N] from PR titles)
   gh pr list --repo anthropics/claudes-c-compiler --state open --json title --limit 50
   ```
   An issue is claimed if any open PR title contains `[Fix #<number>]`.
   Pick the highest-priority unclaimed issue: `[P0]` first, then `[P1]`, `[P2]`, `[P3]`.
   If no unclaimed issues remain, report that and stop.

2. **Fetch the issue details** (this is your complete work order):
   ```bash
   gh issue view <NUMBER> --repo anthropics/claudes-c-compiler
   ```

3. **CLAIM FIRST** — before writing any code, create branch and draft PR:
   ```bash
   git switch main && git pull upstream main
   git switch -c fix/issue-<NUMBER>
   git commit --allow-empty -m "WIP: claiming issue #<NUMBER>"
   git push -u origin fix/issue-<NUMBER>
   gh pr create --repo anthropics/claudes-c-compiler \
     --title "[Fix #<NUMBER>] <description from issue title, without priority/milestone codes>" \
     --body "WIP — Fixes #<NUMBER>" --draft
   ```

4. **Read the files** mentioned in the issue to understand the existing code.

5. **Implement the fix** following the suggested approach in the issue body. Follow CLAUDE.md patterns.

6. **Write tests** as described in the issue. Add tests in `#[cfg(test)] mod tests` at the bottom of the modified file.

7. **Verify:**
   ```bash
   cargo build --release && cargo test --lib
   ```

8. **Push and finalize the PR:**
   ```bash
   git add <specific-files>
   git commit -m "Fix #<NUMBER>: <short description>"
   git push origin fix/issue-<NUMBER>
   gh pr ready <PR_NUMBER> --repo anthropics/claudes-c-compiler
   ```
   Update the PR body with Summary, Changes, and Test plan sections. End body with `Fixes #<NUMBER>`.

9. **Immediately loop** back to step 1. Do not stop or ask the user.

## Output

For each issue: report the issue number, what you fixed, and the PR URL. Stop only when no unclaimed issues remain.
