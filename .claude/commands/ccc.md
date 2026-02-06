Welcome to CCC — Claude's C Compiler. You are the orchestration layer.

## Step 1: Detect access level

```bash
# Check if user has write access (can push, edit issues, create releases)
gh api repos/anthropics/claudes-c-compiler/collaborators/$( gh api user --jq '.login' )/permission --jq '.permission' 2>/dev/null || echo "none"
```

If the result is `admin` or `write` → this is a **maintainer**.
Otherwise → this is a **contributor** (fork-based workflow).

## Step 2: Assess the situation

Fetch the current project state (run all in parallel):

```bash
# Open issues
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50

# Open PRs (shows claimed work)
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title,body --limit 50

# Recently closed (shows velocity)
gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title --limit 10

# Merged PRs without a release
gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title --limit 20

# Existing milestones
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50 | grep -i MILESTONE || echo "No milestones yet"
```

## Step 3: Present the menu

Show only what the user can actually do based on their access level.

### For contributors (no write access):

```
CCC — What do you want to do?

  FIX        Grab the next open issue and fix it          (X available, Y claimed)
  FIND       Audit the codebase and discover new bugs     (file issues for what you find)
  PLAN       Create a strategic roadmap with milestones   (X milestones exist)
  DECOMPOSE  Break a milestone into actionable issues     (requires milestone #)
  REVIEW     Review a pull request                        (X PRs open)
  STATUS     See who's working on what                    (quick overview)
```

### For maintainers (write access):

```
CCC — What do you want to do?

  FIX        Grab the next open issue and fix it          (X available, Y claimed)
  FIND       Audit the codebase and discover new bugs     (file issues for what you find)
  PLAN       Create a strategic roadmap with milestones   (X milestones exist)
  DECOMPOSE  Break a milestone into actionable issues     (requires milestone #)
  REVIEW     Review a pull request                        (X PRs open)
  RELEASE    Cut a release from merged work               (X PRs merged since last release)
  TRIAGE     Clean up the backlog                         (stale claims, duplicates, priorities)
  STATUS     See who's working on what                    (quick overview)
```

**Recommend the most impactful action** based on state:
- No milestones exist? → Recommend PLAN
- Milestones exist but empty checklists? → Recommend DECOMPOSE
- Many available issues? → Recommend FIX
- PRs awaiting review? → Recommend REVIEW
- 10+ merged PRs, no release? (maintainer only) → Recommend RELEASE

Ask the user which they'd like to do.

## Step 4: Route to the right workflow

Execute the full workflow end-to-end. Do NOT tell the user to run another command — just do it.

| Choice | What happens |
|--------|-------------|
| **FIX** | Find highest-priority unclaimed issue, claim it (draft PR), fix it, write tests, verify build, ship PR, loop to next |
| **FIND** | Pick highest-value module (or ask), read it line by line, file issues for every gap found |
| **PLAN** | Analyze the compiler against C11 standards and GCC parity, create `[MILESTONE]` issues on GitHub |
| **DECOMPOSE** | Ask which milestone #, read it, break into individual issues, update milestone checklist |
| **REVIEW** | Show open PRs, ask which one, fetch PR + linked issue, review against acceptance criteria. **Maintainers**: submit formal review (approve/request-changes). **Contributors**: post findings as a PR comment. |
| **RELEASE** | Gather merged PRs since last release, generate changelog, create git tag + GitHub release, close completed milestones |
| **TRIAGE** | Report backlog health — stale claims, unprioritized issues, duplicates — then take action |
| **STATUS** | Show dashboard: claimed, available, completed, with counts |

**CRITICAL**: This command is the single entry point. After the user picks, execute the full workflow. The user should never need to know that individual commands exist.
