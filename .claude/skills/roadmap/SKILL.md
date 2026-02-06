# Roadmap — Strategic Vision and Milestone Planning

Analyze where the CCC compiler is today, identify what's missing, and create a prioritized
roadmap of milestones that drive the project toward production quality.

## When to Use

Use this skill when:
- Starting a new planning cycle (all current milestones are complete or nearly complete)
- You want to assess the compiler's current capabilities vs what's needed
- You need to create strategic direction for contributors

## Workflow

### 1. Assess Current State

**Codebase analysis:**
- Read CLAUDE.md for architecture overview
- Read README.md for claimed capabilities and known limitations
- Check the compilation pipeline: what stages exist, what's missing

**Issue/PR analysis:**
```bash
# What's been done
gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title --limit 50
gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title --limit 50

# What's in progress
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50
gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50

# Existing milestones
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50 \
  | grep -i MILESTONE
```

**Gap analysis — compare against:**
- C11 standard compliance (see [STANDARDS.md](STANDARDS.md))
- GCC/Clang diagnostic parity (what warnings/errors do they catch that CCC doesn't?)
- Real-world build success (what projects fail to build and why?)
- Test coverage (which modules have zero tests?)

### 2. Identify Strategic Gaps

Organize gaps into themes:

| Theme | Example gaps |
|-------|-------------|
| **Diagnostics** | Missing warnings, incorrect error messages, no `-Wpedantic` |
| **Type system** | Incomplete type checking, missing qualifiers, tentative definitions |
| **Optimization** | All -O levels run same pipeline, missing passes |
| **Standards compliance** | C11 features not implemented |
| **Testing** | Modules with zero test coverage, no integration test framework |
| **Tooling** | CLI gaps, missing flags, silent failures |
| **Backend** | Architecture-specific bugs, missing instructions |

### 3. Create Milestones

Each milestone is a GitHub issue with `[MILESTONE]` prefix. Create them in priority order.

**Milestone structure:**

```bash
gh issue create --repo anthropics/claudes-c-compiler \
  --title "[MILESTONE] M<N>: <Name>" \
  --body "$(cat <<'EOF'
## Goal

<1-2 sentences: what this milestone achieves and why it matters>

## Scope

<What's included and what's explicitly excluded>

## Issues

<Checklist of issues — may be empty initially, filled by /decompose>
- [ ] Issue descriptions (to be filed)

## Success criteria

<How to know the milestone is complete — concrete, testable>

## Dependencies

<Other milestones that must be done first, if any>

## Priority

<Why this milestone comes before/after others in the sequence>
EOF
)"
```

**Milestone naming convention:**
- `M1`, `M2`, `M3`... in priority order
- Name should describe the outcome, not the activity
- Examples:
  - `[MILESTONE] M1: Core Diagnostic Coverage` — not "Add warnings"
  - `[MILESTONE] M2: CLI Reliability` — not "Fix CLI bugs"
  - `[MILESTONE] M3: Test Infrastructure` — not "Write tests"

### 4. Sequence Milestones

Order milestones so that:
1. **Foundation first** — test infrastructure before features (you need tests to validate fixes)
2. **Highest impact first** — P0 correctness bugs before P3 nice-to-haves
3. **Dependencies respected** — if M3 depends on M1, M1 comes first
4. **Quick wins early** — build momentum with achievable milestones

### 5. Output

Report the roadmap as:

```
ROADMAP
=======

M1: <Name>                    [X issues, estimated Y effort]
    <one-line description>
    Dependencies: none

M2: <Name>                    [X issues, estimated Y effort]
    <one-line description>
    Dependencies: M1

M3: <Name>                    [X issues, estimated Y effort]
    <one-line description>
    Dependencies: none (parallel with M2)

...

NEXT STEPS:
  Run /decompose M1 to break the first milestone into concrete issues.
```

## Roadmap Refresh

When running roadmap on a project that already has milestones:

1. Check existing `[MILESTONE]` issues — which are complete? which are stale?
2. Close completed milestones
3. Assess what's changed since the last roadmap
4. Create new milestones for the next planning cycle
5. Re-sequence if priorities have shifted

## Reference Files

- **[STANDARDS.md](STANDARDS.md)** — C11 standard sections and CCC's coverage status
- **[BENCHMARKS.md](BENCHMARKS.md)** — Real-world project build results and failure analysis
