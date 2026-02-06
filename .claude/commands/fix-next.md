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
   gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50
   ```
   **CRITICAL: Draft PRs are LOCKS.** An issue is claimed if ANY open PR title contains
   `[Fix #<number>]` — **draft or ready, both count as claimed**. A draft PR means another
   agent is working on it. Do NOT touch it, do NOT try to help. Skip it.

   Pick the highest-priority unclaimed issue: `[P0]` first, then `[P1]`, `[P2]`, `[P3]`.
   If no unclaimed issues remain, report that and stop.

2. **Fetch and VALIDATE the issue** (BEFORE creating any branch or PR):
   ```bash
   gh issue view <NUMBER> --repo anthropics/claudes-c-compiler
   ```

   **Post REVIEWING comment:**
   ```bash
   gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
     --body "<!-- CCC:REVIEWING -->
   **Reviewing** — investigating whether this issue is valid."
   ```

   Read the source files mentioned in the issue. Check if the bug actually exists.

   **If NOT real** → post DENIED with proof, skip to next issue:
   ```bash
   gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
     --body "<!-- CCC:DENIED -->
   **Denied** — <evidence and reasoning>."
   ```
   Go back to step 1.

   **If real** → post CONFIRMED and proceed:
   ```bash
   gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
     --body "<!-- CCC:CONFIRMED -->
   **Confirmed** — <brief explanation>. Proceeding with fix."
   ```

3. **Detect the chain and CLAIM** — after validation confirms issue is real:

   ```bash
   # Detect chain tip (highest non-draft [CC] PR)
   CHAIN_TIP=$(gh pr list --repo anthropics/claudes-c-compiler --state open \
     --json number,title,headRefName,isDraft --limit 100 \
     | jq -r '[.[] | select(.title | test("^\\[CC\\]")) | select(.isDraft | not)] | sort_by(.number) | last')
   CHAIN_TIP_NUMBER=$(echo "$CHAIN_TIP" | jq -r '.number // empty')
   ```

   **If chain exists** (`CHAIN_TIP_NUMBER` is set):
   ```bash
   gh pr checkout $CHAIN_TIP_NUMBER --detach
   git switch -c fix/issue-<NUMBER>
   git commit --allow-empty -m "WIP: claiming issue #<NUMBER>"
   git push -u origin fix/issue-<NUMBER>
   gh pr create --repo anthropics/claudes-c-compiler \
     --title "[CC][Fix #<NUMBER>] <description from issue title, without priority/milestone codes>" \
     --body "WIP — Fixes #<NUMBER>" --draft
   ```

   **If no chain**:
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

8. **Push:**
   ```bash
   git add <specific-files>
   git commit -m "Fix #<NUMBER>: <short description>"
   git push origin fix/issue-<NUMBER>
   ```

9. **CRITICAL — Convert draft PR to ready for review. DO NOT SKIP THIS STEP:**
   ```bash
   gh pr ready <PR_NUMBER> --repo anthropics/claudes-c-compiler
   ```

   Then **write the PR body**. Re-read your diff and the issue body, then write four sections:
   - **Problem** — What was broken, why it matters, C11 reference if applicable, what GCC does
   - **Approach** — Technical decisions, why this approach, alternatives considered
   - **Changes** — Files modified with specific descriptions
   - **Test plan** — One checkbox per behavior verified (not just "tests pass")

   End with `Fixes #<NUMBER>` and milestone link if applicable.
   See [PR_BODY_GUIDE.md](../skills/fix-next/PR_BODY_GUIDE.md) for the full quality standard with good/bad examples.

   **A body that just says "Added check" or lists bullet points is not acceptable.**
   **A draft PR that stays draft is invisible to reviewers. The fix is NOT done until the PR is marked ready.**

   **Note: Once marked ready, your `[CC]` PR becomes the new chain tip. The next loop iteration will detect it and branch off it.**

10. **Post COMPLETE comment on the issue:**
    ```bash
    gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
      --body "<!-- CCC:COMPLETE -->
    **Complete** — fix shipped in PR #<PR_NUMBER>. Awaiting merge."
    ```

11. **Immediately loop** back to step 1. Do not stop or ask the user.

## Output

For each issue: report the issue number, what you fixed, and the PR URL. Stop only when no unclaimed issues remain.
