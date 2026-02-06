Show the current status of all issues and PRs on the CCC compiler project.

## Step 1: Fetch data

Run these commands in parallel:

```bash
# All open issues (includes milestones)
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title,body --limit 100

# All closed issues (for milestone progress)
gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title,body --limit 100

# All open PRs (shows claimed/in-progress work)
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title,body,author,url --limit 50

# Recently merged PRs
gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title,url --limit 20
```

## Step 2: Correlate

- For each open PR, extract the issue number from `Fixes #N` in the body → claimed issues
- For each `[MILESTONE]` issue, find all issues (open + closed) whose body contains `Part of [MILESTONE]` referencing it → milestone progress

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
