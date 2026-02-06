Show the current status of all issues and PRs on the CCC compiler project.

## Step 1: Fetch data

Run these commands in parallel:

```bash
# All issues — titles encode priority, milestone membership, milestone markers
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 100
gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title --limit 100

# Open PRs — titles contain [Fix #N] for claim detection, isDraft for completion state
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title,author,url,isDraft --limit 50

# Merged PRs
gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title,url --limit 20
```

## Step 2: Parse from titles

- **Milestones**: titles matching `[MILESTONE] M<N>:`
- **Milestone sub-issues**: titles containing `[M<N>]` — count per milestone by state
- **Priority**: `[P0]`-`[P3]` in title
- **Complete (awaiting merge)**: open issue + non-draft PR with `[Fix #N]` in title
- **In progress**: open issue + draft PR with `[Fix #N]` in title
- **Available**: open issue, no PR with `[Fix #N]`
- **Merged**: closed issue

## Step 3: Display dashboard

```
CHAIN:
  #19 → #45 → #46                    (3 PRs, tip: #46)
  New [CC] branches base off #46

MILESTONES:
  M1: Core Diagnostic Coverage (#42)    — 6/6 done (0 merged, 6 awaiting merge)
  M2: CLI Reliability (#43)             — needs decomposition (0 sub-issues)

COMPLETE (awaiting merge — non-draft PR exists):
  #20 [P0] Duplicate case labels       ← PR #45 (ready)
  #21 [P0] Duplicate default labels    ← PR #46 (ready)
  ...

IN PROGRESS (draft PR exists):
  #XX [P1] Some issue                  ← PR #XX (draft)

AVAILABLE (unclaimed):
  #36 [P3] Test infrastructure
  ...

MERGED:
  #XX [P0] Some issue                  ← PR #XX merged
  ...

SUMMARY:
  Total: XX | Merged: XX | Complete: XX | In progress: XX | Available: XX
```

Group available issues by priority (P0 first). Show milestones first if any exist.
