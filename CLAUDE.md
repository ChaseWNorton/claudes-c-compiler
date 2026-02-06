# CCC — Claude's C Compiler

## Project overview

A C11 compiler written in Rust targeting x86-64, i686, AArch64, and RISC-V 64.
~188K lines of Rust, zero external dependencies, complete self-contained toolchain
(preprocessor, lexer, parser, sema, SSA IR, 15 optimization passes, code generator,
assembler, linker, DWARF debug info).

## Build & test

```bash
cargo build --release      # ~30s clean build
cargo test --lib           # 602 unit tests
cargo clippy --all-targets # lint check
```

## Contributing with Claude Code

This repo uses a multiplayer coordination system built on skills. Multiple Claude Code
instances can work on different issues simultaneously without conflicts.

### Quick start

1. Fork the repo and clone your fork
2. Set up remotes: `origin` = your fork (pushable), `upstream` = `anthropics/claudes-c-compiler` (read-only)
3. Run **`/ccc`**

That's it. Claude checks your access level, shows the project state, and asks what you want to do. Pick one and the full workflow runs end-to-end. The menu adapts — contributors see what contributors can do, maintainers see the full set.

You never need to memorize individual commands. `/ccc` is the only entry point.

### Git remote convention

Everyone forks. Remotes are:
- `origin` = your fork (pushable)
- `upstream` = `anthropics/claudes-c-compiler` (read-only)

All `git push` goes to `origin`. All PRs go from `origin` to `upstream`.

### Coordination protocol

GitHub Issues and PRs are the shared state — no external tools needed.
All project state is readable from titles alone:

```
[P0] Description                    — standalone issue, priority 0
[P2][M1] Description                — issue belonging to milestone M1
[MILESTONE] M1: Description         — milestone definition
[Fix #20] Description               — PR: claims issue #20
[CC][Fix #20] Description           — PR: chain member + claims issue #20
```

### Issue lifecycle

Issues have lifecycle states tracked via **structured comments** (not title edits —
agents can't modify titles on issues they didn't create). State is derived from comments
and PR signals.

| State | Signal | Meaning |
|-------|--------|---------|
| **Available** | Open issue, no lifecycle comment, no `[Fix #N]` PR | Ready for pickup |
| **Triaged** | Comment with `<!-- CCC:TRIAGED -->` marker | Validated by triage, priority recommended, ready for pickup |
| **Reviewing** | Comment with `<!-- CCC:REVIEWING -->` marker | Agent investigating validity |
| **Claimed (WIP)** | Draft PR with `[Fix #N]` in title | Confirmed real, work in progress |
| **Denied** | Comment with `<!-- CCC:DENIED -->` marker + proof | Not a real bug |
| **Complete** | Ready (non-draft) PR with `[Fix #N]` in title | Fix shipped |

```
Available ──→ Triaged (triage validates) ──→ Claimed/WIP (draft PR) ──→ Complete (PR ready)
         └──→ Reviewing (fix agent) ──→ Claimed/WIP (draft PR) ──→ Complete (PR ready)
                                     └──→ Denied (comment with proof)
```

**Triaged issues skip validation.** If an issue has a `<!-- CCC:TRIAGED -->` comment,
it's already been validated — the fix agent can go straight to claiming.

**CRITICAL: An agent MUST validate an issue BEFORE creating a draft PR** — unless
it's already been triaged. Post a `<!-- CCC:REVIEWING -->` comment, read the code,
confirm the bug exists. If not real, post a `<!-- CCC:DENIED -->` comment with proof.

### PR claim states

| State | How it looks on GitHub |
|-------|----------------------|
| **Available** | Open issue, no open PR title contains `[Fix #N]` |
| **Claimed** | Open PR with `[Fix #N]` in the title (draft OR ready — both count) |
| **Done** | PR merged, issue auto-closed |
| **Abandoned** | Close the PR to release the claim |

**CRITICAL: Draft PRs are LOCKS, not requests for help.**
If ANY open PR title contains `[Fix #N]` — whether draft or ready — issue #N is CLAIMED
by another worker. Do NOT touch it. Do NOT try to finish it. Do NOT open a second PR for
the same issue. SKIP IT and move to the next unclaimed issue.

### PR Chain (`[CC]` protocol)

This is an AI-native workflow. When Claude Code fixes an issue, the resulting PR is correct
or nearly correct — it read the issue body (the full work order), read the source, wrote
tests, and verified the build passes. Waiting for human review before starting the next fix
means idle time and drift. The chain eliminates both.

**Core assumption:** PRs opened by this system are correct. Build forward, don't wait.

Each `[CC]` PR's branch includes all commits from lower-numbered `[CC]` PRs, forming a
linear chain. No merge conflicts. No drift. Work compounds — every fix builds on the last.

**How it works:**
- PRs with `[CC]` at the start of their title are part of the chain
- Chain order = PR number (immutable, monotonic)
- **Chain tip** = highest-numbered non-draft `[CC]` PR
- New fix branches are created off the chain tip (not `main`)
- All `[CC]` PRs target `main` — GitHub auto-shrinks diffs when chain PRs merge

**Detect the chain tip:**
```bash
gh pr list --repo anthropics/claudes-c-compiler --state open \
  --json number,title,headRefName,isDraft --limit 100 \
  | jq -r '[.[] | select(.title | test("^\\[CC\\]")) | select(.isDraft | not)] | sort_by(.number) | last'
```

**Speed merge:** Maintainer can merge just the tip PR to get everything (it contains all
prior chain commits), or merge top-to-bottom for incremental review. Either way, zero conflicts.

**If a chain PR is rejected:** downstream PRs cherry-pick their own commits onto the new tip.
The chain self-heals. See TROUBLESHOOTING.md for recovery steps.

**If no chain exists:** fall back to branching off `main` (standard flow, no `[CC]` prefix).

### The Product Lifecycle

Every role in the product lifecycle is a command backed by a skill.

```
/roadmap → /decompose → Issues → /fix-next → /review-fix → /release
    ^                                                          |
    └──────────────────────────────────────────────────────────┘
```

### Commands

**Start here:**

| Command | What it does |
|---------|-------------|
| **`/ccc`** | **The entry point.** Shows project state, asks what you want to do, runs the workflow. |

**Planning (upstream):**

| Command | What it does |
|---------|-------------|
| `/roadmap` | Analyze the compiler, create milestones, then decompose them into issues |
| `/decompose <milestone>` | Break a specific milestone into issues (called automatically by /roadmap) |
| `/audit [path]` | Deep-dive a module, find every gap, file issues |
| `/file-issue [path]` | File a single well-structured issue |

**Execution:**

| Command | What it does |
|---------|-------------|
| `/pick-issue` | Browse issues, see what's available vs claimed |
| `/fix-issue <N>` | Claim and fix a specific issue |
| `/fix-next` | Auto-cycle: claim, fix, PR, next, repeat |

**Review & release (downstream):**

| Command | What it does |
|---------|-------------|
| `/review-fix <PR>` | Review a PR against its issue's acceptance criteria |
| `/release` | Cut a release — changelog, tag, GitHub release |
| `/triage` | Backlog health, stale claims, prioritization |
| `/issue-status` | Dashboard: claimed, available, completed |

### Skills

Skills provide deep context for each workflow. Claude Code loads them automatically.

| Skill | Purpose | Reference files |
|-------|---------|----------------|
| `roadmap` | Strategic vision and milestone creation | STANDARDS.md, BENCHMARKS.md |
| `decompose` | Break milestones into actionable issues | — |
| `audit` | Systematic codebase analysis | AUDIT_AREAS.md |
| `file-issue` | Bug discovery and issue creation | TEMPLATES.md |
| `fix-next` | Auto-cycle issue fixing | COORDINATION.md, CODEBASE_PATTERNS.md, TROUBLESHOOTING.md |
| `review-fix` | PR review against acceptance criteria | CHECKLIST.md |
| `release` | Cut releases from merged work | — |
| `triage` | Backlog health and prioritization | — |

### Milestones

Milestones are GitHub issues with `[MILESTONE]` prefix. They define the goal and success
criteria. They're write-once — no one needs to edit them after creation.

Sub-issues link back to their milestone:
```
Part of [MILESTONE] M1: Core Diagnostic Coverage (#42)
```

Progress is computed dynamically by querying which sub-issues are open vs closed.
No checklist to maintain, no edit permissions needed.

## Code conventions

- Tests go in `#[cfg(test)] mod tests` at the bottom of each file
- Use existing test helpers for frontend tests:
  - `sema_error_count("C code")` — returns error count from semantic analysis
  - `sema_warning_count("C code")` — returns warning count
  - `compile_to_ir("C code")` — returns `IrModule` for IR-level inspection
  - `compile_at_opt_level("C code", N)` — compiles through full pipeline at given opt level
- Follow existing patterns: look at nearby code before adding new abstractions
- No external dependencies — everything is self-contained
- Error messages should match GCC/Clang format where possible
- Cite C11 standard sections in comments for spec-compliance work (e.g., "C11 6.7.3")

## Architecture

```
src/
├── common/          # Shared: types (CType/IrType), error/diagnostic engine, symbol table
├── driver/          # CLI parsing (cli.rs), compilation pipeline (pipeline.rs)
├── frontend/
│   ├── preprocessor/ # Text-based preprocessor with macro expansion
│   ├── lexer/        # Tokenizer (scan.rs)
│   ├── parser/       # Recursive descent parser with precedence climbing
│   └── sema/         # Semantic analysis: type checking, diagnostics, const eval
├── ir/
│   ├── lowering/     # AST → alloca-IR lowering (~18K lines)
│   └── mem2reg/      # Alloca promotion to SSA (promote.rs, phi_eliminate.rs)
├── passes/          # 15 SSA optimization passes (simplify, constfold, dce, gvn, licm, etc.)
└── backend/         # Code generation, assemblers, linkers
    ├── x86/          # x86-64 backend (codegen, assembler, linker)
    ├── i686/         # 32-bit x86 backend
    ├── arm/          # AArch64 backend
    └── riscv/        # RISC-V 64 backend
```

## Key patterns for contributors

**Adding a new diagnostic (warning/error):**
1. If it's a new warning category, add a `WarningKind` variant in `src/common/error.rs`
2. Wire it into `flag_name()`, `from_flag_name()`, `wall_set()`, and `all()`
3. Add the check in `src/frontend/sema/analysis.rs`
4. Emit via `self.diagnostics.borrow_mut().warning_with_kind(msg, span, kind)` or `.error(msg, span)`
5. Write tests using `sema_error_count()` / `sema_warning_count()` helpers

**Adding a new optimization pass:**
1. Create the pass in `src/passes/`
2. Wire it into `run_passes()` in `src/passes/mod.rs`
3. Add a disable flag in the `DisabledPasses` struct
4. Write tests that build IR, run the pass, and assert transformations

**The compilation pipeline:**
```
C source → preprocess → lex → parse → sema → lower (AST→IR) → mem2reg → run_passes → phi-eliminate → codegen → assemble → link
```

`mem2reg` and `phi-eliminate` are required for correctness. `run_passes` is optional (skipped at -O0).
