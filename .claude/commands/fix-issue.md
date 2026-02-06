Work on fixing GitHub issue #$ARGUMENTS from the anthropics/claudes-c-compiler repository.

## Git remote convention

- `origin` = your fork (pushable)
- `upstream` = anthropics/claudes-c-compiler (read-only)

## Pre-flight: Check if already claimed

Check PR titles for `[Fix #$ARGUMENTS]`:
```bash
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50
```
If any PR title contains `[Fix #$ARGUMENTS]` — **draft or ready, both count** — the issue is **LOCKED** by another worker. A draft PR is a claim lock, NOT a request for help. Tell the user and suggest picking another issue. Do NOT try to contribute to the existing PR.

## Phase 0: Validate the issue (BEFORE creating any branch or PR)

1. **Read the issue body** — this is your work order:
   ```bash
   gh issue view $ARGUMENTS --repo anthropics/claudes-c-compiler
   ```

2. **Mark as REVIEWING:**
   ```bash
   gh issue edit $ARGUMENTS --repo anthropics/claudes-c-compiler \
     --title "[REVIEWING]<rest of title without [OPEN]>"
   gh issue comment $ARGUMENTS --repo anthropics/claudes-c-compiler \
     --body "Reviewing: investigating whether this issue is valid."
   ```

3. **Check if the bug actually exists** — read the source files, run a quick test if possible.

4. **If NOT real** → mark DENIED with proof, tell the user:
   ```bash
   gh issue edit $ARGUMENTS --repo anthropics/claudes-c-compiler \
     --title "[DENIED]<rest of title without [REVIEWING]>"
   gh issue comment $ARGUMENTS --repo anthropics/claudes-c-compiler \
     --body "Denied — <evidence and reasoning>."
   ```
   Do NOT create a branch or PR. Suggest picking another issue.

5. **If real** → mark WIP and proceed:
   ```bash
   gh issue edit $ARGUMENTS --repo anthropics/claudes-c-compiler \
     --title "[WIP]<rest of title without [REVIEWING]>"
   gh issue comment $ARGUMENTS --repo anthropics/claudes-c-compiler \
     --body "Confirmed — <brief explanation>. Proceeding with fix."
   ```

## Phase 0.5: Detect the chain

Find the latest non-draft `[CC]` PR to base your branch on:

```bash
CHAIN_TIP=$(gh pr list --repo anthropics/claudes-c-compiler --state open \
  --json number,title,headRefName,isDraft --limit 100 \
  | jq -r '[.[] | select(.title | test("^\\[CC\\]")) | select(.isDraft | not)] | sort_by(.number) | last')
CHAIN_TIP_NUMBER=$(echo "$CHAIN_TIP" | jq -r '.number // empty')
```

If `CHAIN_TIP_NUMBER` is empty, no chain exists — base off `main`.

## Phase 1: Claim (before writing any code)

1. **Fetch the issue title** for the PR:
   ```bash
   gh issue view $ARGUMENTS --repo anthropics/claudes-c-compiler --json title --jq '.title'
   ```

2. **Create branch and draft PR** (chain-aware):

   **If chain exists** (`CHAIN_TIP_NUMBER` is set):
   ```bash
   gh pr checkout $CHAIN_TIP_NUMBER --detach
   git switch -c fix/issue-$ARGUMENTS
   git commit --allow-empty -m "WIP: claiming issue #$ARGUMENTS"
   git push -u origin fix/issue-$ARGUMENTS
   gh pr create --repo anthropics/claudes-c-compiler \
     --title "[CC][Fix #$ARGUMENTS] <description from issue title without priority codes>" \
     --body "$(cat <<'EOF'
   WIP — implementing fix.

   ## Chain
   - **Based on**: #CHAIN_TIP_NUMBER

   Fixes #$ARGUMENTS
   EOF
   )" --draft
   ```

   **If no chain**:
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

9. **CRITICAL — Convert draft PR to ready for review. DO NOT SKIP THIS STEP:**
   ```bash
   gh pr ready <PR_NUMBER> --repo anthropics/claudes-c-compiler
   ```
   **A draft PR that stays draft is invisible to reviewers. The fix is NOT done until the PR is marked ready.**

10. **Mark issue COMPLETE:**
    ```bash
    gh issue edit $ARGUMENTS --repo anthropics/claudes-c-compiler \
      --title "[COMPLETE]<rest of title without [WIP]>"
    gh issue comment $ARGUMENTS --repo anthropics/claudes-c-compiler \
      --body "Complete — fix shipped in PR #<PR_NUMBER>. Awaiting merge."
    ```

11. **Write the PR body** — this is as important as the code. Read [PR_BODY_GUIDE.md](../skills/fix-next/PR_BODY_GUIDE.md) for the full quality standard.

    Before writing, re-read your diff and the issue body. Then write a body with these four sections:

    - **Problem** — What was broken, why it matters, C11 reference if applicable, what GCC does
    - **Approach** — Technical decisions, why this approach, alternatives considered
    - **Changes** — Files modified with specific descriptions
    - **Test plan** — One checkbox per behavior verified (not just "tests pass")

    End with `Fixes #$ARGUMENTS` and the milestone link if applicable.

    **A body that just says "Added check" or "Fixed the bug" is not acceptable. Write for a reviewer who hasn't read the issue.**

## PR requirements

- Title: `[CC][Fix #$ARGUMENTS] <description>` (if chain) or `[Fix #$ARGUMENTS] <description>` (if no chain)
- Body has four sections: Problem, Approach, Changes, Test plan (see [PR_BODY_GUIDE.md](../skills/fix-next/PR_BODY_GUIDE.md))
- Body ends with: `Fixes #$ARGUMENTS`
- All existing tests pass + new tests for the fix
- Clean build with no new warnings
