# Coordination Protocol

Detailed reference for the multiplayer claim/release system using GitHub as shared state.

## Git remote convention

- `origin` = your fork (pushable)
- `upstream` = anthropics/claudes-c-compiler (read-only)

All `git push` goes to `origin`. All PRs go from `origin` to `upstream`.

## Contents

- Issue lifecycle
- State model (PR claims)
- PR chain
- Claim protocol (step by step)
- Release and abandonment
- Race conditions and conflict resolution
- Stale claim detection

## Issue Lifecycle

Issue lifecycle is tracked entirely via **comments** — agents cannot edit issue titles
they didn't create. State is derived from comments posted on the issue and from PR signals.

| State | How to detect | Meaning |
|-------|--------------|---------|
| **Available** | Open issue, no `<!-- CCC:REVIEWING -->` comment, no `[Fix #N]` PR | Ready for pickup |
| **Reviewing** | `<!-- CCC:REVIEWING -->` comment exists on the issue | Agent is investigating validity |
| **Confirmed / WIP** | `<!-- CCC:CONFIRMED -->` comment + draft PR with `[Fix #N]` | Bug is real, fix in progress |
| **Denied** | `<!-- CCC:DENIED -->` comment with proof | Not a real bug |
| **Complete** | Ready (non-draft) PR with `[Fix #N]` | Fix shipped, awaiting merge |

### Flow

```
Available ──→ Reviewing (comment) ──→ Confirmed + draft PR ──→ Complete (PR ready)
                                   └──→ Denied (comment with proof)
```

### Lifecycle comments

All lifecycle comments use HTML comment markers for machine detection. The marker
goes on its own line at the top. The human-readable text follows.

**Post REVIEWING** (before any code work):
```bash
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "$(cat <<'EOF'
<!-- CCC:REVIEWING -->
**Reviewing** — investigating whether this issue is valid. Checking the code now.
EOF
)"
```

**Post CONFIRMED** (bug is real, about to create draft PR):
```bash
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "$(cat <<'EOF'
<!-- CCC:CONFIRMED -->
**Confirmed** — <brief explanation of why it's real>. Creating draft PR to claim.
EOF
)"
```

**Post DENIED** (not a real issue — requires proof):
```bash
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "$(cat <<'EOF'
<!-- CCC:DENIED -->
**Denied** — this is not a valid issue.

## Evidence
<code references, test output, reasoning>

## Recommendation
Close this issue. <or: refile as a different issue if the underlying concern is valid>
EOF
)"
```

**Post COMPLETE** (fix shipped, PR marked ready):
```bash
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "$(cat <<'EOF'
<!-- CCC:COMPLETE -->
**Complete** — fix shipped in PR #<PR_NUMBER>. Awaiting merge.
EOF
)"
```

### Detecting lifecycle state

To check an issue's current state before picking it up:
```bash
gh api repos/anthropics/claudes-c-compiler/issues/<NUMBER>/comments \
  --jq '.[].body' | grep -o 'CCC:[A-Z]*' | tail -1
```
This returns the latest state marker (e.g., `CCC:REVIEWING`, `CCC:DENIED`).
If empty, the issue has never been reviewed — it's available.

### Rules

1. **BEFORE creating a draft PR**, the agent MUST:
   - Read the issue body completely
   - Post a `<!-- CCC:REVIEWING -->` comment
   - Verify the bug exists (check the code, run a test if possible)
   - If confirmed real → post `<!-- CCC:CONFIRMED -->` comment + create draft PR
   - If not real → post `<!-- CCC:DENIED -->` comment with proof, NO PR

2. **Every state change** = a new comment with the appropriate marker.

3. **Denied requires proof** — code references, test output, or reasoning.
   Never deny without evidence.

4. **Skip issues that already have a `CCC:REVIEWING` or `CCC:DENIED` comment** —
   someone else is already handling it or has already rejected it.

## State Model

Every issue exists in exactly one state:

| State | Signal on GitHub | What it means |
|-------|-----------------|---------------|
| **Available** | Open issue, no open PR title contains `[Fix #N]` | No one is working on it |
| **Claimed** | Open PR (draft OR ready) with `[Fix #N]` in title | Someone is actively working — **DO NOT TOUCH** |
| **Done** | PR merged; issue auto-closed by GitHub | Fix is complete |
| **Abandoned** | PR closed without merge | Claim released, issue available again |

**CRITICAL: A draft PR is a LOCK, not a request for help.**
When you see `[Fix #N]` in ANY open PR title — draft or ready — that issue is taken.
Another agent created that draft PR to claim the issue before writing code. It is their
workspace. Do NOT read it, do NOT try to contribute to it, do NOT open a competing PR.
Skip it immediately and move to the next unclaimed issue.

The state transitions are:

```
Available ──claim──→ Claimed ──merge──→ Done
                        │
                        └──close PR──→ Available (abandoned)
```

## PR Chain

When PRs build on each other's work, they form a linear chain. Each PR's branch
includes all commits from every earlier chain PR. The `[CC]` prefix in PR titles
identifies chain membership.

```
main → [CC] PR #19 (infra) → [CC][Fix #20] PR #45 → [CC][Fix #21] PR #46 → ...
```

### Chain state

| State | Signal |
|-------|--------|
| **Chain tip** | Highest-numbered non-draft `[CC]` PR |
| **In chain** | Open `[CC]` PR |
| **No chain** | No `[CC]` PRs exist — fall back to `main` |

### Detecting the chain tip

```bash
gh pr list --repo anthropics/claudes-c-compiler --state open \
  --json number,title,headRefName,isDraft --limit 100 \
  | jq -r '[.[] | select(.title | test("^\\[CC\\]")) | select(.isDraft | not)] | sort_by(.number) | last'
```

This returns the chain tip PR (number, title, branch name). If empty, no chain exists.

### Chain ordering

Ordered by PR number (ascending). Each `[CC]` PR's branch must include all commits
from lower-numbered `[CC]` PRs. This invariant is maintained by always branching off
the current chain tip.

### Why `[CC]`?

Follows the existing title-code convention (`[P0]`, `[M1]`, `[Fix #N]`). Detectable
from titles alone. Opt-in: PRs that don't build on the chain omit `[CC]`.

### Speed merge

The maintainer can merge top-to-bottom (each merge is trivial because the branch
includes everything already in `main`). Or merge just the chain tip to get everything
at once.

## Claim Protocol

### Step 1: Discover unclaimed work

```bash
# Get all open issues
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50

# Get all open PRs — titles contain [Fix #N] for claim detection
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50
```

An issue is **claimed** if any open PR title contains `[Fix #<number>]` — **draft or ready, both count as claimed**.
A draft PR is a lock held by another worker. Do NOT touch claimed issues.
Subtract claimed from open issues. Pick the highest-priority unclaimed one.

Priority sort: `[P0]` first, then `[P1]`, `[P2]`, `[P3]`, then unprefixed.
Title codes: `[P<N>]` = priority, `[M<N>]` = milestone membership (informational, doesn't affect priority).

### Step 2: Base off the chain tip (or main)

```bash
# Detect the chain tip
CHAIN_TIP=$(gh pr list --repo anthropics/claudes-c-compiler --state open \
  --json number,title,headRefName,isDraft --limit 100 \
  | jq -r '[.[] | select(.title | test("^\\[CC\\]")) | select(.isDraft | not)] | sort_by(.number) | last')
CHAIN_TIP_NUMBER=$(echo "$CHAIN_TIP" | jq -r '.number // empty')
```

If chain exists, check out the chain tip. If not, use main:

```bash
if [ -n "$CHAIN_TIP_NUMBER" ]; then
  gh pr checkout $CHAIN_TIP_NUMBER --detach
else
  git switch main && git pull upstream main
fi
```

**Never branch off another worker's fix branch directly** — always go through chain detection.

### Step 3: Validate the issue (BEFORE creating any branch or PR)

```bash
# Read the issue body — this is the work order
gh issue view <NUMBER> --repo anthropics/claudes-c-compiler
```

**Post a REVIEWING comment:**
```bash
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "<!-- CCC:REVIEWING -->
**Reviewing** — investigating whether this issue is valid."
```

Read the source files mentioned in the issue. Check if the bug actually exists.
Run a quick test if possible.

**If the issue is NOT real — post DENIED with proof:**
```bash
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "<!-- CCC:DENIED -->
**Denied** — <evidence and reasoning>. Recommend closing."
```
Skip this issue and go back to Step 1. Do NOT create a branch or PR.

**If the issue IS real — post CONFIRMED, proceed to Step 4:**
```bash
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "<!-- CCC:CONFIRMED -->
**Confirmed** — <brief explanation>. Proceeding with fix."
```

### Step 4: Create branch

```bash
git switch -c fix/issue-<NUMBER>
```

Branch naming convention: `fix/issue-<NUMBER>`. This makes it easy to identify which issue a branch belongs to.

### Step 5: Create draft PR (= the claim)

```bash
git commit --allow-empty -m "WIP: claiming issue #<NUMBER>"
git push -u origin fix/issue-<NUMBER>
```

If chain exists (CHAIN_TIP_NUMBER is set), add `[CC]` to the title:

```bash
# Chain-aware PR creation
gh pr create --repo anthropics/claudes-c-compiler \
  --title "[CC][Fix #<NUMBER>] <description>" \
  --body "$(cat <<'EOF'
## Summary
Work in progress — implementing fix.

## Chain
- **Based on**: #<CHAIN_TIP_NUMBER>

## Changes
(will be updated when complete)

## Test plan
(will be updated when complete)

Fixes #<NUMBER>
EOF
)" --draft
```

If no chain, standard PR creation (no `[CC]` prefix):

```bash
gh pr create --repo anthropics/claudes-c-compiler \
  --title "[Fix #<NUMBER>] <description>" \
  --body "WIP — Fixes #<NUMBER>" --draft
```

The moment this PR exists, other workers will see `[Fix #<NUMBER>]` in the title and skip this issue.

### Step 6: Do the work

Read the issue body, read the source files, implement the fix, write tests. See [CODEBASE_PATTERNS.md](CODEBASE_PATTERNS.md) for per-category guidance.

### Step 7: Verify

```bash
cargo build --release && cargo test --lib
```

Both must pass with zero failures before proceeding.

### Step 8: Commit, push, finalize

```bash
git add <specific-files>
git commit -m "Fix #<NUMBER>: <short description>"
git push origin fix/issue-<NUMBER>
```

### Step 9: CRITICAL — Mark PR ready, update issue to COMPLETE, write body

**DO NOT SKIP THIS. A draft PR is invisible to reviewers. The fix is NOT done until you run this:**

```bash
gh pr ready <PR_NUMBER> --repo anthropics/claudes-c-compiler
```

**Post COMPLETE comment on the issue:**
```bash
gh issue comment <NUMBER> --repo anthropics/claudes-c-compiler \
  --body "<!-- CCC:COMPLETE -->
**Complete** — fix shipped in PR #<PR_NUMBER>. Awaiting merge."
```

Then **write the PR body**. This is as important as the code itself.

Re-read your diff and the issue body. Then write a body with four sections:

1. **Problem** — What was broken, why it matters, C11 reference if applicable, what GCC does
2. **Approach** — Technical decisions you made, why this approach, alternatives considered
3. **Changes** — Files modified with specific descriptions of what changed in each
4. **Test plan** — One checkbox per behavior verified, plus build/test confirmation

End with `Fixes #<NUMBER>` and the milestone link if applicable.

See [PR_BODY_GUIDE.md](PR_BODY_GUIDE.md) for the full quality standard with good/bad examples.

**A body that just says "Added check" or lists bullet points without context is not acceptable.
Write for a reviewer who hasn't read the issue.**

## Release and Abandonment

### Voluntary release (you want to stop working on it)

Close the draft PR. This removes the `[Fix #N]` signal from PR titles and makes the issue available again.

```bash
gh pr close <PR_NUMBER> --repo anthropics/claudes-c-compiler
```

### Detecting stale claims

A claim is **stale** if the draft PR has had no commits or updates for an extended period (e.g., 24+ hours). To check:

```bash
# List open draft PRs with their last update time
gh pr list --repo anthropics/claudes-c-compiler --state open --draft \
  --json number,title,updatedAt --limit 50
```

Look at `updatedAt`. If a draft PR hasn't been updated in >24 hours and is still draft, the worker likely crashed or abandoned it. Close the PR to release the claim:

```bash
gh pr close <PR_NUMBER> --repo anthropics/claudes-c-compiler \
  --comment "Releasing stale claim — no activity for 24+ hours. Issue is available for others."
```

## Race Conditions

### Two workers claim the same issue simultaneously

This is unlikely but possible. Both create draft PRs with `[Fix #N]` at nearly the same time.

**Detection**: When you run the claim check and find an existing PR for your issue that isn't yours:
```bash
gh pr list --repo anthropics/claudes-c-compiler --state open \
  --json title,url,author --jq '.[] | select(.title | test("\\[Fix #<NUMBER>\\]"))'
```

**Resolution**: If you see another PR already claiming the issue, close yours and move on:
```bash
gh pr close <YOUR_PR_NUMBER> --repo anthropics/claudes-c-compiler \
  --comment "Duplicate claim — another worker got here first."
```

The rule is simple: **first PR created wins**. If in doubt, check PR creation timestamps.

### Worker crashes mid-fix

The draft PR remains open (claim is held). The issue appears claimed to other workers.

**Self-recovery**: If you restart and want to continue your work:
```bash
git switch fix/issue-<NUMBER>
git pull origin fix/issue-<NUMBER>
# Continue working
```

**Release for others**: If you won't continue, close the PR to release the claim.

## Pre-flight Check

Before starting any issue, always verify it's not already claimed:

```bash
gh pr list --repo anthropics/claudes-c-compiler --state open \
  --json title,url --jq '.[] | select(.title | test("\\[Fix #<NUMBER>\\]")) | .url'
```

If this returns a URL, someone else is working on it. Pick a different issue.
