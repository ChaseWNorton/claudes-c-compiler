Show the current status of all issues and PRs on the CCC compiler project.

## Step 1: Fetch data

Run these commands in parallel:

```bash
# All issues — titles encode priority, milestone membership, milestone markers
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 100
gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title --limit 100

# Open PRs — titles contain [Fix #N] for claim detection
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title,author,url --limit 50

# Merged PRs
gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title,url --limit 20
```

## Step 2: Parse from titles

- **Milestones**: titles matching `[MILESTONE] M<N>:`
- **Milestone sub-issues**: titles containing `[M<N>]` — count open vs closed per milestone
- **Priority**: `[P0]`-`[P3]` in title
- **Claimed**: open PR title contains `[Fix #<issue_number>]`

## Step 3: Display dashboard

```
MILESTONES:
  M1: Core Diagnostic Coverage (#42)    — 2/6 done
  M2: CLI Reliability (#43)             — needs decomposition (0 sub-issues)

CLAIMED (in progress):
  #20 [P0] Duplicate case labels       ← PR #XX by @author
  #21 [P0] Duplicate default labels    ← PR #XX by @author

AVAILABLE (unclaimed):
  #22 [P0] Void function return
  #23 [P0] case outside switch
  ...

RECENTLY COMPLETED:
  #XX [P0] Some issue                  ← PR #XX merged
  ...

SUMMARY:
  Total open: XX | Claimed: XX | Available: XX | Completed: XX
```

Group available issues by priority (P0 first). Show milestones first if any exist.
