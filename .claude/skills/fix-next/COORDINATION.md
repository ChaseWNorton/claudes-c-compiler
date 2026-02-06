# Coordination Protocol

Detailed reference for the multiplayer claim/release system using GitHub as shared state.

## Git remote convention

- `origin` = your fork (pushable)
- `upstream` = anthropics/claudes-c-compiler (read-only)

All `git push` goes to `origin`. All PRs go from `origin` to `upstream`.

## Contents

- State model
- PR chain
- Claim protocol (step by step)
- Release and abandonment
- Race conditions and conflict resolution
- Stale claim detection

## State Model

Every issue exists in exactly one state:

| State | Signal on GitHub | What it means |
|-------|-----------------|---------------|
| **Available** | Open issue, no open PR title contains `[Fix #N]` | No one is working on it |
| **Claimed** | Open PR with `[Fix #N]` in title | Someone is actively working |
| **Done** | PR merged; issue auto-closed by GitHub | Fix is complete |
| **Abandoned** | PR closed without merge | Claim released, issue available again |

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

An issue is **claimed** if any open PR title contains `[Fix #<number>]`.
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

### Step 3: Create branch

```bash
git switch -c fix/issue-<NUMBER>
```

Branch naming convention: `fix/issue-<NUMBER>`. This makes it easy to identify which issue a branch belongs to.

### Step 4: Create draft PR (= the claim)

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

### Step 5: Do the work

Read the issue body, read the source files, implement the fix, write tests. See [CODEBASE_PATTERNS.md](CODEBASE_PATTERNS.md) for per-category guidance.

### Step 6: Verify

```bash
cargo build --release && cargo test --lib
```

Both must pass with zero failures before proceeding.

### Step 7: Commit, push, finalize

```bash
git add <specific-files>
git commit -m "Fix #<NUMBER>: <short description>"
git push origin fix/issue-<NUMBER>
```

### Step 8: CRITICAL — Mark PR ready for review

**DO NOT SKIP THIS. A draft PR is invisible to reviewers. The fix is NOT done until you run this:**

```bash
gh pr ready <PR_NUMBER> --repo anthropics/claudes-c-compiler
```

Then update the body:

```bash
gh pr edit <PR_NUMBER> --repo anthropics/claudes-c-compiler --body "$(cat <<'EOF'
## Summary
<what was wrong and why — reference C11 section if applicable>

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
