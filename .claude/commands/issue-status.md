Show the current status of all issues and PRs on the CCC compiler project.

## Step 1: Fetch data

Run these commands in parallel:

```bash
# All open issues
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50

# All open PRs (shows claimed/in-progress work)
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title,body,author,url --limit 50

# Recently closed issues (completed work)
gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title --limit 20

# Recently merged PRs
gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title,url --limit 20
```

## Step 2: Correlate issues with PRs

For each open PR, extract the issue number from `Fixes #N` in the body. This tells you which issues are claimed.

## Step 3: Display dashboard

Format the output as:

```
CLAIMED (in progress):
  #20 [P0] Duplicate case labels       ← PR #XX by @author
  #21 [P0] Duplicate default labels    ← PR #XX by @author

AVAILABLE (unclaimed):
  #22 [P0] Void function return        — small effort
  #23 [P0] case outside switch          — small effort
  ...

RECENTLY COMPLETED:
  #XX [P0] Some issue                  ← PR #XX merged
  ...

SUMMARY:
  Total open: XX | Claimed: XX | Available: XX | Completed: XX
```

Group available issues by priority (P0 first).
