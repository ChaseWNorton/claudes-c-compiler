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

### Quick start (single issue)

1. Fork the repo and clone your fork
2. Run `/pick-issue` to see available issues (shows what's claimed vs open)
3. Run `/fix-issue <number>` to claim and fix a specific issue
4. Your draft PR is the claim — other workers will see it and skip that issue

### Auto-cycle mode (fix everything)

Run `/fix-next` to enter auto-cycle mode. Claude Code will:
1. Find the highest-priority unclaimed issue
2. Claim it (draft PR)
3. Implement the fix and write tests
4. Mark PR ready for review
5. Move to the next unclaimed issue
6. Repeat until no work remains

### Coordination protocol

GitHub Issues and PRs are the shared state — no external tools needed.

| State | How it looks on GitHub |
|-------|----------------------|
| **Available** | Open issue, no open PR references it |
| **Claimed** | Open draft PR with `Fixes #N` in the body |
| **Done** | PR merged, issue auto-closed |
| **Abandoned** | Close the draft PR to release the claim |

### The Product Lifecycle

Every role in the product lifecycle is a command backed by a skill.

```
/roadmap → /decompose → Issues → /fix-next → /review-fix → /release
    ^                                                          |
    └──────────────────────────────────────────────────────────┘
```

### Commands

**Planning (upstream):**

| Command | What it does |
|---------|-------------|
| `/roadmap` | Analyze the compiler, create strategic milestones |
| `/decompose <milestone>` | Break a milestone into concrete issues |
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

Milestones are GitHub issues with `[MILESTONE]` prefix and checklists. GitHub auto-checks
boxes when linked issues close. No special permissions needed — anyone can create them.

```
[MILESTONE] M1: Core Diagnostic Coverage
  - [ ] #20 Duplicate case labels
  - [x] #25 Fix sema TODO comment    ← auto-checked when #25 closes
```

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
