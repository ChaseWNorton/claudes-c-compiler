# CCC Team Guide — Working Together with /ccc

## The One Thing You Need to Understand

**The chain tip is the real repo. Not `main`.**

`main` has zero merged PRs — it's the original codebase. The actual current state of the compiler lives on the chain tip: the highest-numbered ready `[CC]` PR. That branch contains every fix, every new diagnostic, every test that's been written. It's 40+ commits ahead of `main`.

Every workflow — fixing, triaging, auditing, planning, reviewing — operates against the chain tip, because that's where the code actually is. `/ccc` handles this automatically.

---

## Setup (5 minutes)

### 1. Fork and clone

```bash
# Fork anthropics/claudes-c-compiler on GitHub, then:
git clone git@github.com:YOUR_USERNAME/claudes-c-compiler.git
cd claudes-c-compiler
```

### 2. Set up remotes

```bash
git remote add upstream https://github.com/anthropics/claudes-c-compiler.git
```

**Rule: `origin` = your fork (you push here). `upstream` = the canonical repo (PRs target here). You'll accumulate more remotes as you work — that's by design.**

### 3. Bootstrap: get onto the chain

**This is critical.** The `/ccc` command and all the skills live in the chain — they don't exist on `main`. A fresh clone of the upstream repo has none of the `.claude/commands/` or `.claude/skills/` files. You need to get onto the chain tip before `/ccc` will work.

The chain tip can be on any contributor's fork. Run this to find it and sync up:

```bash
# Find the chain tip: whose fork, which branch
CHAIN_INFO=$(gh pr list --repo anthropics/claudes-c-compiler --state open \
  --json number,title,headRefName,isDraft,headRepositoryOwner --limit 100 \
  | jq '[.[] | select(.title | test("^\\[CC\\]")) | select(.isDraft | not)]
        | sort_by(.number) | last')

OWNER=$(echo "$CHAIN_INFO" | jq -r '.headRepositoryOwner.login')
BRANCH=$(echo "$CHAIN_INFO" | jq -r '.headRefName')

# Add their fork as a remote and fetch
git remote add "$OWNER" "https://github.com/$OWNER/claudes-c-compiler.git"
git fetch "$OWNER" "$BRANCH"

# Get on the chain
git switch -c working "$OWNER/$BRANCH"
```

After this, your local tree has all the skills, all the fixes, and everything you need.

### 4. Open Claude Code and type `/ccc`

Now it works. Claude syncs you to the chain tip, shows project state, and asks what you want to do. From here on, `/ccc` handles remote additions automatically when the chain tip moves to a different fork.

---

## How the Chain Moves Between Forks

This is the key thing that makes multiplayer work — and the thing most likely to confuse you at first.

The chain tip can be on **anyone's fork**. When Chase fixes an issue, the tip is on `ChaseWNorton/claudes-c-compiler`. When you fix the next issue, the tip moves to `YOUR_USERNAME/claudes-c-compiler`. When someone else picks up after you, the tip moves to their fork.

```
PR #183 chain tip → branch on ChaseWNorton's fork
You fix #150      → PR #204 chain tip → branch on YOUR fork
Person C fixes    → PR #205 chain tip → branch on PersonC's fork
```

**You need the chain tip owner's fork as a git remote to branch off it.** `/ccc` handles this automatically:

```bash
# What /ccc does under the hood (you don't run this manually):

# 1. Find chain tip + whose fork it's on
CHAIN_TIP=$(gh pr list --repo anthropics/claudes-c-compiler --state open \
  --json number,title,headRefName,isDraft,headRepositoryOwner --limit 100 \
  | jq '[.[] | select(.title | test("^\\[CC\\]")) | select(.isDraft | not)]
        | sort_by(.number) | last')

OWNER=$(echo "$CHAIN_TIP" | jq -r '.headRepositoryOwner.login')
BRANCH=$(echo "$CHAIN_TIP" | jq -r '.headRefName')

# 2. Add their fork as a remote if we don't have it yet
if ! git remote get-url "$OWNER" &>/dev/null; then
    git remote add "$OWNER" "https://github.com/$OWNER/claudes-c-compiler.git"
fi

# 3. Fetch and work off it
git fetch "$OWNER" "$BRANCH"
```

Over time, your remotes accumulate — one per person who's contributed to the chain:

```
origin         = your fork (push here)
upstream       = anthropics/claudes-c-compiler (PRs target here)
ChaseWNorton   = added when chain tip was on Chase's fork
PersonB        = added when chain tip moved to Person B's fork
PersonC        = added when chain tip moved to Person C's fork
```

This is normal. Remotes are cheap. `/ccc` adds them as needed.

---

## Why Chain Tip = Source of Truth (For Everything)

It's not just FIX that needs the chain tip. **Every role does:**

| Role | Why it needs chain-tip code |
|------|-----------------------------|
| **FIX** | Branch off tip to continue the chain. |
| **TRIAGE** | Must check if a reported bug still exists in current code. A bug fixed in PR #180 but still open on `main` would be a false positive. |
| **AUDIT** | Reading `main` would find gaps that are already fixed. Wastes everyone's time. |
| **PLAN** | Strategic priorities depend on what's actually missing, not what was missing 40 fixes ago. |
| **FIND** | Same as audit — need current code to find real gaps. |
| **REVIEW** | Need context of surrounding code as it exists now. |
| **FILE ISSUE** | Must verify bug exists in current code, not just in stale `main`. |

`/ccc` syncs to the chain tip before presenting the menu. Your local working tree reflects the real state of the compiler, regardless of which role you pick.

---

## The Chain

When someone fixes an issue, their PR includes all commits from every prior fix. This forms a linear chain:

```
main
  └─ PR #178 [CC][Fix #134] hex escapes in #if          ← on ChaseWNorton's fork
       └─ PR #179 [CC][Fix #135] #elif in inactive blocks
            └─ PR #180 [CC][Fix #136] variadic detection
                 └─ ...
                      └─ PR #202 [CC][Fix #152] error attr  ← chain tip (current)
```

**Chain tip** = highest-numbered non-draft `[CC]` PR. All new work branches off here.

**Speed merge:** The maintainer can merge just the tip PR to get everything at once (it contains all prior commits), or merge one-by-one for incremental review. Either way, zero conflicts.

---

## Draft PRs Are Locks

When someone starts working on issue #N, Claude Code creates a **draft PR** titled `[CC][Fix #N] ...`. This tells every other session: **this issue is taken. Skip it.**

If `[Fix #N]` appears in any open PR title — draft or ready — that issue is claimed. Period. Don't touch it, don't try to help, don't open a second PR. Move on.

---

## Issue Lifecycle

Issues move through states tracked by structured comments (HTML markers):

```
Available         →  no lifecycle comment, no PR
Triaged           →  <!-- CCC:TRIAGED --> comment (validated, ready for pickup)
Reviewing         →  <!-- CCC:REVIEWING --> comment (agent investigating)
Claimed (WIP)     →  draft PR with [Fix #N] in title
Complete          →  ready (non-draft) PR with [Fix #N]
Denied            →  <!-- CCC:DENIED --> comment (not a real bug)
Decomposed        →  <!-- CCC:DECOMPOSED --> comment (broken into sub-issues)
```

**Triaged issues skip validation.** If triage already confirmed a bug is real, the fix agent goes straight to claiming — no need to re-validate.

---

## What Each Role Does

### FIX — Grab an issue and fix it

The core loop. Claude Code:
1. Syncs to chain tip (adds remote if needed)
2. Finds the highest-priority unclaimed issue
3. Validates it (unless already triaged)
4. Claims it (draft PR = lock)
5. Reads the source, implements the fix, writes tests
6. Verifies `cargo build --release && cargo test --lib`
7. Marks PR ready, writes the PR body
8. **Your PR becomes the new chain tip**
9. Moves to the next issue automatically

**This is the default. Most sessions should be doing FIX.**

### TRIAGE — Validate external issues

Read issues filed by external contributors. Check if the bug exists **in chain-tip code** (not `main`). Post `CCC:TRIAGED` with a priority recommendation, or `CCC:DENIED` with proof it's not real. Identify duplicates and stale claims.

### FIND — Audit the codebase for bugs

Read a module line by line **in the chain-tip code**. Find gaps — missing diagnostics, silent failures, panics on user input. File issues for each finding. Feeds the backlog that FIX consumes.

### PLAN — Create roadmap and milestones

Analyze the compiler **as it currently exists** (chain tip). Identify strategic gaps, create `[MILESTONE]` issues, decompose them into concrete sub-issues ready for workers.

### REVIEW — Review a pull request

Fetch a PR and its linked issue, check the fix against acceptance criteria, verify tests cover the claims, check for regressions.

### RELEASE — Cut a release (maintainers only)

Gather merged PRs, generate changelog, create git tag + GitHub release.

### STATUS — Dashboard

Quick overview of milestones, chain status, claimed/available/completed counts.

---

## Multiple People Working Simultaneously

### Scenario: Two people start at the same time

- **Person A** runs `/ccc` → FIX. Claude picks issue #150, creates draft PR.
- **Person B** runs `/ccc` → FIX one minute later. Claude sees `[Fix #150]` is claimed. **Skips it.** Picks #149 instead.

No conflict. No coordination needed.

### Scenario: Chain tip is on someone else's fork

Person A's chain tip PR is on their fork. Person B runs `/ccc`. Claude detects the tip is on Person A's fork, adds it as a remote, fetches the branch, and creates Person B's fix branch off it. Person B's PR becomes the new tip — now on Person B's fork. Person C will add Person B's fork when they start.

**The chain migrates between forks automatically.**

### Scenario: One person audits while another fixes

FIND creates issues. FIX consumes issues. They pipeline naturally — one feeds the other. No conflicts because they operate on different GitHub objects.

### Scenario: Triage and FIX run simultaneously

Triage validates and prioritizes issues. FIX picks the highest-priority unclaimed issue. If triage marks something `[P0]`, FIX will grab it next. They complement each other.

### Scenario: A chain PR gets rejected

Downstream PRs cherry-pick their own commits onto the new tip. The chain self-heals. (Rare — the assumption is that Claude Code PRs are correct.)

---

## Priority System

Issues are prioritized in their titles:

| Priority | Meaning | Examples |
|----------|---------|---------|
| `[P0]` | Crash, hang, or miscompilation on valid C | Panics, infinite loops, wrong codegen |
| `[P1]` | Missing validation causing silent wrong behavior | Accepts invalid code, wrong sizeof value |
| `[P2]` | Feature gaps, non-critical correctness | Missing warnings, stub builtins |
| `[P3]` | Quality of life, testing, documentation | Doctest failures, missing tests |

FIX always picks the highest-priority unclaimed issue first.

---

## Reading the State at a Glance

Everything is in titles:

```
Issue:  [P1][M8] Function pointer indirect calls don't compute return ABI
        ^^^^      ← priority 1
            ^^^^  ← belongs to milestone 8

PR:     [CC][Fix #112] Compute return ABI for function pointer calls
        ^^^^            ← part of the chain
            ^^^^^^^^^^  ← claims issue #112 (LOCKED — don't touch)
```

- Open issue + no `[Fix #N]` PR = **available**
- Open issue + `[Fix #N]` PR exists = **claimed, skip it**
- Closed issue = **done** (merged PR auto-closed it)

---

## The Commands (You Don't Need These)

`/ccc` is the only entry point. But the individual commands exist if you want them:

| Command | What |
|---------|------|
| `/ccc` | **Start here.** Syncs to chain tip, shows state, runs workflow. |
| `/fix-next` | Auto-cycle: claim, fix, PR, next, repeat |
| `/fix-issue <N>` | Fix a specific issue |
| `/pick-issue` | Browse available issues |
| `/review-fix <PR>` | Review a specific PR |
| `/file-issue` | File a single issue |
| `/audit [path]` | Deep audit a module |
| `/roadmap` | Strategic planning |
| `/triage` | Backlog health |
| `/issue-status` | Dashboard |
| `/release` | Cut a release |

---

## Quick Reference: Your First Session

1. Fork + clone + add `upstream` remote (once)
2. `claude` (open Claude Code)
3. `/ccc`
4. Claude syncs to chain tip (adds remotes as needed), shows you the state
5. Pick a role — FIX, TRIAGE, FIND, PLAN, REVIEW
6. The full workflow runs end-to-end

You don't manage branches. You don't figure out whose fork to fetch from. You don't check which issues are claimed. `/ccc` handles all of it.
