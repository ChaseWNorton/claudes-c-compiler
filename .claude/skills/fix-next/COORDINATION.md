# Coordination Protocol

Detailed reference for the multiplayer claim/release system using GitHub as shared state.

## Contents

- State model
- Claim protocol (step by step)
- Release and abandonment
- Race conditions and conflict resolution
- Stale claim detection

## State Model

Every issue exists in exactly one state:

| State | Signal on GitHub | What it means |
|-------|-----------------|---------------|
| **Available** | Open issue, no open PR body contains `Fixes #N` | No one is working on it |
| **Claimed** | Open PR (usually draft) with `Fixes #N` in body | Someone is actively working |
| **Done** | PR merged; issue auto-closed by GitHub | Fix is complete |
| **Abandoned** | PR closed without merge | Claim released, issue available again |

The state transitions are:

```
Available ──claim──→ Claimed ──merge──→ Done
                        │
                        └──close PR──→ Available (abandoned)
```

## Claim Protocol

### Step 1: Discover unclaimed work

```bash
# Get all open issues
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50

# Get all issue numbers already claimed by open PRs
gh pr list --repo anthropics/claudes-c-compiler --state open --json body --jq '.[].body' \
  | grep -oP 'Fixes #\K[0-9]+' | sort -u
```

Subtract claimed from open issues. Pick the highest-priority unclaimed one.

Priority sort: `[P0]` first, then `[P1]`, `[P2]`, `[P3]`, then unprefixed.
Title codes: `[P<N>]` = priority, `[M<N>]` = milestone membership (informational, doesn't affect priority).

### Step 2: Switch to clean main

```bash
git switch main
git pull origin main
```

Always start from a fresh main. Never branch off another fix branch.

### Step 3: Create branch

```bash
git switch -c fix/issue-<NUMBER>
```

Branch naming convention: `fix/issue-<NUMBER>`. This makes it easy to identify which issue a branch belongs to.

### Step 4: Create draft PR (= the claim)

```bash
git commit --allow-empty -m "WIP: Fix #<NUMBER>: <title>"
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

The moment this PR exists, other workers will see `Fixes #<NUMBER>` and skip this issue.

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
git push
```

### Step 8: Mark PR ready + update body

```bash
gh pr ready <PR_NUMBER> --repo anthropics/claudes-c-compiler

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

Close the draft PR. This removes the `Fixes #N` signal and makes the issue available again.

```bash
gh pr close <PR_NUMBER> --repo anthropics/claudes-c-compiler
```

### Detecting stale claims

A claim is **stale** if the draft PR has had no commits or updates for an extended period (e.g., 24+ hours). To check:

```bash
# List open draft PRs with their last update time
gh pr list --repo anthropics/claudes-c-compiler --state open --draft \
  --json number,title,updatedAt,body --limit 50
```

Look at `updatedAt`. If a draft PR hasn't been updated in >24 hours and is still draft, the worker likely crashed or abandoned it. Close the PR to release the claim:

```bash
gh pr close <PR_NUMBER> --repo anthropics/claudes-c-compiler \
  --comment "Releasing stale claim — no activity for 24+ hours. Issue is available for others."
```

## Race Conditions

### Two workers claim the same issue simultaneously

This is unlikely but possible. Both create draft PRs with `Fixes #N` at nearly the same time.

**Detection**: When you run the claim check and find an existing PR for your issue that isn't yours:
```bash
gh pr list --repo anthropics/claudes-c-compiler --state open \
  --json body,url,author --jq '.[] | select(.body | test("Fixes #<NUMBER>\\b"))'
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
  --json body,url --jq '.[] | select(.body | test("Fixes #<NUMBER>\\b")) | .url'
```

If this returns a URL, someone else is working on it. Pick a different issue.
