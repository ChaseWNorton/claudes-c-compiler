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
# Open issues (includes milestones — look for [MILESTONE] in title)
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title,body --limit 100

# Open PRs (shows claimed work — look for "Fixes #N" in body)
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title,body --limit 50

# Closed issues (for milestone progress — sub-issues may be closed)
gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title,body --limit 100

# Merged PRs (for release scope)
gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title --limit 20
```

### Compute milestone progress

For each open issue with `[MILESTONE]` in the title:
1. Get its issue number (e.g., #42)
2. Search ALL issues (open + closed) for bodies containing `Part of [MILESTONE]` referencing #42
3. Count open vs closed → that's the progress

A milestone with sub-issues is **decomposed**. A milestone with zero sub-issues **needs decomposition**. A milestone where all sub-issues are closed is **complete**.

## Step 3: Present the menu

Show only what the user can actually do based on their access level.

### For contributors (no write access):

```
CCC — What do you want to do?

  FIX        Grab the next open issue and fix it          (X available, Y claimed)
  FIND       Audit the codebase and discover new bugs     (file issues for what you find)
  PLAN       Create a strategic roadmap with milestones   (X milestones exist)
  REVIEW     Review a pull request                        (X PRs open)
  STATUS     See who's working on what                    (quick overview)
```

### For maintainers (write access):

```
CCC — What do you want to do?

  FIX        Grab the next open issue and fix it          (X available, Y claimed)
  FIND       Audit the codebase and discover new bugs     (file issues for what you find)
  PLAN       Create a strategic roadmap with milestones   (X milestones exist)
  REVIEW     Review a pull request                        (X PRs open)
  RELEASE    Cut a release from merged work               (X PRs merged since last release)
  TRIAGE     Clean up the backlog                         (stale claims, duplicates, priorities)
  STATUS     See who's working on what                    (quick overview)
```

If milestones exist, show their progress inline:
```
  MILESTONES:
    M1: Core Diagnostic Coverage — 2/6 done    (4 available to fix)
    M2: CLI Reliability — needs decomposition   (0 sub-issues filed)
```

**Recommend the most impactful action** based on state:
- No milestones exist? → Recommend PLAN
- Milestones exist but not decomposed (zero sub-issues)? → Recommend PLAN (it will auto-decompose)
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
| **PLAN** | Full planning flow — see below |
| **REVIEW** | Show open PRs, ask which one, fetch PR + linked issue, review against acceptance criteria. **Maintainers**: formal review (approve/request-changes). **Contributors**: post findings as a PR comment. |
| **RELEASE** | Gather merged PRs since last release, generate changelog, create git tag + GitHub release, close completed milestones |
| **TRIAGE** | Report backlog health — stale claims, unprioritized issues, duplicates — then take action |
| **STATUS** | Show dashboard with milestone progress, claimed/available/completed counts |

### PLAN flow (auto-chains)

PLAN is not a single step — it's a pipeline:

1. **Roadmap** — Analyze the compiler, identify strategic gaps, create `[MILESTONE]` issues
2. **Decompose** — For each milestone just created, immediately break it into sub-issues. Each sub-issue links back via `Part of [MILESTONE] M<N> (#number)` in its body.
3. **Report** — Show what was created: milestones, sub-issues per milestone, and recommend FIX

Do NOT stop after creating milestones. Do NOT ask the user to run a separate command.
The user said PLAN — that means milestones AND their sub-issues, ready for workers.

If milestones already exist and have undecomposed ones (zero sub-issues), decompose those.
If all milestones are fully decomposed, do a roadmap refresh — assess what's changed, create new milestones if needed.

**CRITICAL**: This command is the single entry point. After the user picks, execute the full workflow. The user should never need to know that individual commands exist.
