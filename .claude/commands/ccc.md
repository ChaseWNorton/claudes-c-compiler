Welcome to CCC — Claude's C Compiler. You are the orchestration layer.

## Title codes

All project state is readable from titles alone — issues, PRs, milestones:

```
[P0][M1] Description                — issue: priority P0, milestone M1
[P2] Description                    — issue: priority P2, standalone
[MILESTONE] M1: Description         — milestone definition
[Fix #20] Description               — PR: claims issue #20
```

Codes: `[P<N>]` priority, `[M<N>]` milestone membership, `[MILESTONE]` milestone marker, `[Fix #<N>]` PR claim.

## Git remote convention

Everyone forks. Remotes are:
- `origin` = your fork (pushable)
- `upstream` = `anthropics/claudes-c-compiler` (read-only)

All `git push` goes to `origin`. All PRs go from `origin` to `upstream`.

## Step 1: Detect access level

```bash
gh api repos/anthropics/claudes-c-compiler/collaborators/$( gh api user --jq '.login' )/permission --jq '.permission' 2>/dev/null || echo "none"
```

`admin` or `write` → **maintainer**. Otherwise → **contributor** (fork-based).

## Step 2: Assess the situation

Titles only — no bodies needed for routing:

```bash
# Issues (open + closed)
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 100
gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title --limit 100

# PRs (open + merged) — titles contain [Fix #N] for claim detection
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50
gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title --limit 20
```

### Parse from titles

- **Issue priority**: `[P0]`-`[P3]` in issue title
- **Milestone membership**: `[M<N>]` in issue title
- **Milestone definitions**: `[MILESTONE] M<N>:` in issue title
- **Claims**: `[Fix #<N>]` in PR title — issue #N is claimed
- **Unprioritized**: issue title has no `[P<N>]` code → needs triage

### Compute milestone progress

For each open `[MILESTONE]` issue, count `[M<N>]` issues across open + closed titles.

- Zero `[M<N>]` issues → needs decomposition
- Some open, some closed → in progress (X/Y)
- All closed → complete

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
    M2: CLI Reliability — needs decomposition   (0 sub-issues)
```

**Recommend the most impactful action** based on state:
- No milestones exist? → Recommend PLAN
- Milestones with zero sub-issues? → Recommend PLAN (it will auto-decompose)
- Many available issues? → Recommend FIX
- PRs awaiting review? → Recommend REVIEW
- 10+ merged PRs, no release? (maintainer only) → Recommend RELEASE

Ask the user which they'd like to do.

## Step 4: Route to the right workflow

Execute the full workflow end-to-end. Do NOT tell the user to run another command — just do it.

| Choice | What happens |
|--------|-------------|
| **FIX** | Claim first (draft PR), then fix. See FIX flow below. |
| **FIND** | Pick highest-value module (or ask), read it line by line, file issues for every gap found |
| **PLAN** | Full planning flow — see PLAN flow below |
| **REVIEW** | Show open PRs, ask which one, fetch PR + linked issue, review against acceptance criteria. **Maintainers**: formal review. **Contributors**: PR comment. |
| **RELEASE** | Gather merged PRs since last release, generate changelog, create git tag + GitHub release, close completed milestones |
| **TRIAGE** | Report backlog health — stale claims, unprioritized issues, duplicates — then take action |
| **STATUS** | Show dashboard with milestone progress, claimed/available/completed counts |

### FIX flow

The claim and the fix are separate phases:

1. **Claim** — create branch, push to `origin` (your fork), open draft PR to `upstream` with title `[Fix #<N>] <description>`. This is the lock. Do this BEFORE writing any code.
2. **Fix** — read the issue body (work order), read the source files, implement the fix, write tests, verify build.
3. **Ship** — commit, push, mark PR ready, update PR body with summary/changes/test plan.
4. **Loop** — go back to step 1 with the next unclaimed issue.

### PLAN flow (auto-chains)

1. **Roadmap** — Analyze the compiler, identify strategic gaps, create `[MILESTONE] M<N>:` issues
2. **Decompose** — For each milestone, break into sub-issues titled `[P<N>][M<N>] <description>`.
3. **Report** — Show what was created, recommend FIX

Do NOT stop after creating milestones. The user said PLAN — that means milestones AND their sub-issues, ready for workers.

**CRITICAL**: This command is the single entry point. After the user picks, execute the full workflow. The user should never need to know that individual commands exist.
