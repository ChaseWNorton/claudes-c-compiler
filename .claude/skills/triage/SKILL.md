# Triage — Backlog Management

Analyze, prioritize, and manage the issue backlog for CCC.

## When to Use

Use this skill when:
- New issues have been filed and need prioritization
- You want to audit the backlog for stale or duplicate issues
- You want to check overall project health and progress
- You need to re-prioritize based on new information

## Triage Workflow

### 1. Fetch current state

```bash
# All issues — titles encode everything: priority, milestone membership, milestone markers
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 100
gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title --limit 100

# Open PRs — titles contain [Fix #N] for claim detection, isDraft for completion state
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title,author,updatedAt,isDraft --limit 50

# Merged PRs
gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title --limit 20
```

### 1b. Parse from titles

- **Priority**: `[P0]`-`[P3]` in title. Unprefixed → needs triage.
- **Milestones**: titles matching `[MILESTONE] M<N>:`
- **Milestone sub-issues**: titles containing `[M<N>]` — count open vs closed per milestone
- **Claimed**: open PR title contains `[Fix #<number>]`

### 2. Categorize issues

Group open issues by:

**Priority** (from title prefix):
- `[P0]` Critical — blocks correctness
- `[P1]` High — important for reliability
- `[P2]` Medium — feature gaps
- `[P3]` Low — nice to have
- Unprefixed — needs triage

**Status** (cross-reference PRs with `isDraft`):
- **Available** — no open PR title contains `[Fix #N]`
- **In progress** — draft PR with `[Fix #N]` in title
- **Complete** — non-draft PR with `[Fix #N]` in title (awaiting merge)
- **Merged** — issue closed (PR merged)
- **Stale claim** — draft PR with `[Fix #N]` but no activity in 24+ hours

**Category** (from issue body):
- Frontend diagnostics (sema)
- CLI / driver
- Backend / codegen
- Testing infrastructure
- Documentation

### 3. Identify action items

**Issues needing triage** (unprefixed titles):
- Read the issue body
- Assign a priority prefix
- Suggest: `gh issue edit <NUMBER> --repo anthropics/claudes-c-compiler --title "[P<N>] <title>"`

**Stale claims**:
- Check if the PR has any real commits (not just the WIP empty commit)
- If no progress in 24+ hours, comment on the PR asking for status
- If no response in 48+ hours, close the PR to release the claim

**Duplicate issues**:
- Compare issue descriptions for overlap
- Close the duplicate with a comment linking to the original
- `gh issue close <NUMBER> --repo anthropics/claudes-c-compiler --comment "Duplicate of #XX"`

**Issues that are unclear or missing information**:
- Comment asking for clarification
- If the issue was filed by the file-issue skill, it should be complete — check against the template

### 4. Report

Format the output as:

```
BACKLOG HEALTH REPORT
=====================

NEEDS TRIAGE (no priority assigned):
  #XX <title>

STALE CLAIMS (no activity in 24+ hours):
  #XX <title> — PR #YY by @author, last updated <date>

DUPLICATES DETECTED:
  #XX and #YY — <description>

MILESTONES:
  M1: Core Diagnostic Coverage — 6/6 done (0 merged, 6 awaiting merge)
      Merged: (none) | Complete: #20, #21, #22, #23, #24, #25 | Available: (none)

PROGRESS:
  Total: XX | Merged: XX | Complete: XX | In progress: XX | Available: XX

BY PRIORITY:
  P0: XX open (XX claimed, XX available)
  P1: XX open (XX claimed, XX available)
  P2: XX open (XX claimed, XX available)
  P3: XX open (XX claimed, XX available)

RECOMMENDATIONS:
  - <suggested actions>
```

## Priority Assignment Guide

When triaging an unprefixed issue, assign priority based on:

| Priority | Criteria |
|----------|----------|
| **P0** | Incorrect code accepted silently, miscompilation, crash on valid code |
| **P1** | Missing validation that could cause confusing failures, important warnings |
| **P2** | Feature gaps that limit what C code can be compiled, non-critical correctness |
| **P3** | Testing gaps, documentation, developer experience, tooling |

**When in doubt**: P2 is the safe default. Upgrade to P1 if it could cause silent bugs. Upgrade to P0 if valid C code is silently miscompiled.

## Stale Claim Policy

A claim (draft PR) is considered stale when:
- It's been 24+ hours since the last update
- The PR has no real commits (only the initial empty WIP commit)
- The author hasn't commented or pushed

Action:
1. Comment on the PR asking for status
2. Wait 24 more hours
3. If no response, close the PR with a comment explaining why
4. The issue becomes available again for others

## Maintenance Tasks

Run periodically to keep the backlog healthy:

- **Weekly**: Check for stale claims
- **After a batch of merges**: Verify closed issues are actually fixed
- **Before starting a new work session**: Run `/issue-status` to see the current picture
