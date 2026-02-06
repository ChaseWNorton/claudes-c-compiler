Review the open issues on this repository and help me pick one to work on.

## Step 1: Fetch open issues and open PRs

Run both of these commands:

```bash
# All open issues
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title,body --limit 50

# All open PRs (to detect claimed issues)
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title,body --limit 50
```

## Step 2: Determine which issues are claimed

An issue is **claimed** if any open PR body contains `Fixes #<number>`. Parse the PR bodies to build a set of claimed issue numbers.

## Step 3: Present the issues

Parse title codes: `[P0]`-`[P3]` = priority, `[M<N>]` = milestone membership.

Group by priority:
- **P0 (Critical)** — correctness bugs that any C compiler should catch
- **P1 (High)** — important bugs and infrastructure gaps
- **P2 (Medium)** — correctness and feature gaps
- **P3 (Low)** — testing and nice-to-have improvements

For each issue show:
- Issue number and title
- Milestone (if `[M<N>]` is in the title)
- Status: **AVAILABLE** or **CLAIMED** (with link to the PR)
- A one-line summary of effort required (small/medium/large)

## Step 4: Recommend

Suggest the highest-priority **available** (unclaimed) issue. Then ask which issue the user wants to work on.

If they pick one, tell them to run `/fix-issue <number>` to start.
