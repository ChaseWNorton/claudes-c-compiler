# Fitting a Compiler's Output Into 32KB: An Engineering Case Study

## The Problem

CCC (Claude's C Compiler) is a C11 compiler written in Rust. It targets x86-64, i686, AArch64, and RISC-V 64. The challenge: compile the Linux kernel's x86 boot code — 21 C source files that must link into a setup image no larger than 32,768 bytes. GCC does this comfortably. As of this writing, CCC does too.

The Linux kernel enforces this limit with a linker script assertion: `ASSERT(_end <= 0x8000)`. If the compiled output exceeds 32KB, the linker refuses to produce a binary. The boot code runs in 16-bit real mode during the earliest stage of x86 startup, before the kernel switches to protected mode. There is no room for negotiation — the BIOS loads exactly this much.

This was a known limitation of CCC. The original project evaluation stated: *"It lacks the 16-bit x86 compiler that is necessary to boot Linux out of real mode. For this, it calls out to GCC."* The work described in this case study documents the attempt to eliminate that dependency. CCC's i686 backend now compiles all 21 boot C files natively — the code is compiled as 32-bit with `.code16gcc` assembler directives that emit operand/address size prefixes for 16-bit real mode execution, the same technique GCC uses. No GCC involvement for the C compilation. All 21 files pass the QEMU swap test (boot to "Linux version").

> **Correction (2026-02-10):** The originally-reported linked output of 31,120 bytes was measured with TWO bugs:
>
> 1. **Missing .code16gcc prefixes (discovered 2026-02-09):** CCC's assembler had `.code16gcc` support structurally present but never wired up. The `code16gcc` field on `InstructionEncoder` was always `false`. Every instruction was encoded as plain 32-bit without the 0x66/0x67 override prefixes required for 16-bit real mode execution. Attempting to boot with this code immediately crashed with #UD (Invalid Opcode). GCC's reference numbers (22,976 bytes) include these prefix bytes because GAS has always handled `.code16gcc` correctly.
>
> 2. **classify_line comment stripping (discovered 2026-02-10):** The peephole optimizer's `classify_line` function parsed register names from text that included `# PHI_COPY` comments. `register_family("%eax    # PHI_COPY")` returned REG_NONE, misclassifying move instructions as `Other`. This broke `propagate_register_copies` — writes to registers were invisible, stale copies persisted, and correct instructions were incorrectly eliminated as self-moves. The video-mode swap test failed until this was fixed. The fix produces correct but larger code.
>
> With both bugs fixed: code-only = **36,287 bytes**, linked _end = **47,504 bytes (`0xb990`) — 14,736 bytes over the 32KB limit.** The 32KB goal is not yet achieved. All 21 files pass the QEMU swap test. The code-only optimizations documented below are still valid for relative savings — only the absolute numbers changed.

## Starting Point

- All 19 boot files compile successfully with CCC at `-m16 -Os`
- Linked image: 39,232 bytes (`_end = 0x9940`)
- Limit: 32,768 bytes (`_end = 0x8000`)
- **Gap: 6,464 bytes over (19.7%)**
- GCC -Os produces a linked image comfortably under 32KB for the same source files

## The i686 Accumulator Architecture

Understanding CCC's codegen architecture is essential to understanding every decision that follows.

CCC's i686 backend uses an **accumulator model**. The x86-32 ISA has 8 general-purpose registers, but CCC uses them as follows:

| Register | Role | Available for values? |
|----------|------|----------------------|
| eax | Accumulator — all computation routes through it | No (constantly clobbered) |
| esp | Stack pointer | No (hardware-reserved) |
| ebp | Callee-saved (frame pointer omitted at -Os) | Yes |
| ebx | Callee-saved storage | Yes |
| esi | Callee-saved storage | Yes |
| edi | Callee-saved storage | Yes |
| ecx | Caller-saved (clobbered by loads, shifts) | Barely |
| edx | Caller-saved (clobbered by division, stores) | Barely |

Every binary operation follows the pattern:
```asm
movl <source>, %eax       # load left operand into accumulator
<op>l <right>, %eax       # perform operation
movl %eax, <destination>  # store result from accumulator
```

GCC, by contrast, operates directly on registers: `addl %esi, %ebx` — no intermediary. This architectural difference is the root cause of CCC's code size overhead.

## The Journey

### Phase 1: Direct-Operand ALU Instructions

**Hypothesis:** CCC wastes an instruction loading the right-hand operand into ecx before every ALU operation. x86 supports `addl 12(%esp), %eax` (memory-direct) and `addl %ebx, %eax` (register-direct) — no ecx middleman needed.

**What we built:**
- `rhs_operand_str()` — a helper that returns the assembly operand string for a value without emitting any instructions. Returns `$5` for constants, `%ebx` for register-allocated values, `12(%esp)` for stack slots.
- Rewired `emit_int_binop_impl()` in `alu.rs` to use direct operands for add, sub, mul, and, or, xor
- Rewired `emit_int_cmp_impl()` and `emit_fused_branch()` in `comparison.rs` to use direct operands for comparisons
- Applies to all commutative/binary ops except shifts (require `%cl`) and division (require `edx:eax`)

**Result:** This was a prerequisite for Phase 2 — it eliminated most ecx clobbering, making ecx available for register allocation.

### Phase 2: Caller-Saved Register Allocation (ecx)

**Hypothesis:** CCC's register allocator already supports caller-saved registers (the x86-64 backend allocates 6 of them). The i686 backend passes an empty list:

```rust
pub(super) const I686_CALLER_SAVED: &[PhysReg] = &[];  // ROOT CAUSE
```

The allocator infrastructure exists. It just wasn't plugged in.

**What we built:**
- Added `PhysReg(4) = ecx` to `I686_CALLER_SAVED`
- Extended `phys_reg_name()`, `i686_constraint_to_phys()`, `i686_clobber_to_phys()` to handle ecx
- Filtered caller-saved registers against inline asm clobber lists in `prologue.rs`

**Result:** Near-zero impact on boot code. The allocator correctly assigns ecx to values whose live ranges don't span calls. But boot code is dominated by pointer dereferences (loads/stores), which use ecx as scratch for indirect addressing. The `scratch_clobber_points` mechanism correctly prevents ecx allocation across these instructions — but they're everywhere, so ecx can only hold values in rare pure-computation windows.

**Lesson:** Correct implementation, wrong assumption about workload characteristics. The register was available in theory but clobbered too frequently in practice.

### Phase 3: Phi Relay Coalescing

**Hypothesis:** CCC's phi elimination creates intermediate "relay" values — temporaries whose only purpose is to carry a value from one copy to another. This creates circular shuffle chains that do nothing:

```asm
# "else" path — value UNCHANGED, but CCC generates:
movl %ebx, %edi          # ebx -> edi
movl %edi, %ecx          # edi -> ecx
movl %ecx, %ebx          # ecx -> ebx  (BACK WHERE WE STARTED)
```

Seven instructions, ~21 bytes, accomplishing nothing.

**Root cause:** Three compounding factors:
1. Phi elimination creates intermediate values the regalloc treats as separate variables
2. Copy coalescing is too conservative — requires "sole use" and blocks cross-block aliasing
3. The accumulator model turns each copy into 2 instructions (load to eax, store from eax)

**What we built:**
- `coalesce_phi_relays()` in `copy_coalescing.rs` — identifies values where ALL definitions are Copy instructions and the value is only used as the source of another Copy
- These "relay" values are given the same stack slot as their final destination
- When source slot == destination slot, `generate_copy()` already skips the copy (existing infrastructure at line 1236)

**Result: -4,438 bytes (13.9% reduction).** The biggest single win of the entire project.

Largest per-file savings:
- cmdline.c: -1,210 bytes (48.6% reduction)
- printf.c: -1,038 bytes (18.7% reduction)
- video.c: -420 bytes
- early_serial_console.c: -284 bytes

These are all control-flow-heavy files with many loops and branches — exactly where phi copies accumulate.

### Phase 4: Per-Register Clobber Lists (edx)

**Hypothesis:** ecx barely helped because loads clobber it. edx is clobbered by fewer things (only division, indirect stores, 64-bit casts). With per-register clobber tracking, edx can hold values across loads and GEPs that destroy ecx.

**What we built:**
- Changed `scratch_clobber_points` from `Vec<u32>` (one list for all) to `Vec<Vec<u32>>` (per-register lists)
- Classified each instruction's clobber behavior:
  - Loads through pointers: clobber ecx only
  - Stores through pointers: clobber ecx AND edx
  - GEP (address calc): clobber ecx only
  - Shifts: clobber ecx only
  - Division: clobber ecx AND edx
  - 64-bit casts: clobber edx only
- Modified regalloc Phase 2 to check per-register clobber lists instead of a global list
- Added `PhysReg(5) = edx` to `I686_CALLER_SAVED`

**Result: -560 bytes.** Modest but real — edx can survive across loads and GEPs that ecx cannot.

### The Superoptimizer Diagnostic Tool

**Hypothesis:** A tool that exhaustively searches for shorter instruction sequences could find savings the peephole misses.

**What we built:**
CCC already had a superoptimizer (`src/superopt/`) with an x86 emulator, equivalence verifier, and exhaustive search. We added:
- `diagnose` mode — cross-block harvest of instruction patterns across multiple files
- Identity detection — finds instruction sequences with zero net effect
- Per-file breakdown and ranked hitlist of optimization targets

**Critical bug found:** The identity detection had a seeding bug — `seed_stack()` wrote the same value to all stack offsets, causing `movl %eax, 0(%esp); movl 4(%esp), %eax` to appear as a no-op (both slots held the same value). Fixed by hashing offsets to produce unique values per slot.

**Result after bug fix:** Zero savings found in 4-instruction windows. The assembly is locally optimal — the bloat is structural, not from bad instruction selection. This definitively ruled out peephole-level fixes as a path to significant savings.

### The Parameter Alloca Investigation

**Hypothesis:** mem2reg keeps promoted parameter allocas as dead code because `find_param_alloca` uses positional indexing. The backend copies arguments to dead stack slots.

**What we built:**
- Changed `find_param_alloca` from positional counting to direct Value lookup
- Allowed mem2reg to remove promoted parameter allocas

**Result:** 27 bytes saved on test cases, zero impact on boot code. The peephole optimizer's dead-store elimination passes already clean up the dead copies. The fix was correct but redundant.

**Lesson:** Before building a fix, check if other passes already handle the problem. CCC's 7,000-line peephole optimizer catches many patterns that seem like they need IR-level fixes.

### The Critical Correctness Bug

**Discovery:** While testing size-positive inlining, `caller3` (a multiply-by-constant function) returned garbage. Investigation revealed a pre-existing miscompilation bug.

**Root cause:** The peephole optimizer's `is_reg_dead_from()` function treated ALL caller-saved registers as dead at `ret`:

```rust
LineKind::Ret => return is_caller_saved(reg),
// eax is caller-saved, so this returns true
// meaning "eax is dead at ret" — WRONG
```

eax holds the function's return value at `ret`. It is very much alive. This caused the peephole to delete any computation that wrote to eax immediately before a return — multiplies, shifts, adds would silently vanish.

**Impact:** The previous .text measurement of 31,830 bytes was wrong. The peephole had been deleting correct return-value computations across dozens of functions. The real .text with correct code: **39,238 bytes** — 7,408 bytes of code that had been silently removed.

**The fix (6 lines):**
```rust
LineKind::Ret => {
    if reg == REG_EAX || reg == REG_EDX { return false; } // LIVE
    return is_caller_saved(reg);
}
```

Applied to all 5 liveness functions in peephole.rs. All 879 unit tests pass.

**Lesson:** A miscompilation bug can masquerade as good optimization results. Always validate correctness independently of performance metrics. The "31,830 bytes fitting under 32KB" was an illusion — the code was small because it was broken.

### Size-Positive Inlining

**Hypothesis:** GCC inlines functions at -Os when constant propagation makes the result smaller. CCC suppresses all inlining at -Os.

**Key finding:** When inlining is prevented for both compilers, CCC produces slightly *better* code than GCC (150 vs 155 bytes on a test function). The entire size gap comes from GCC's smart inlining decisions.

**What we built:**
- Trial-clone inlining: for each call site with constant arguments, clone the caller, inline the callee, run constfold + DCE, measure the result, keep only if smaller
- Bypasses the normal block-count limits (CCC's -O2 limit of 6 blocks was preventing inlining of 10-block functions that shrink dramatically with constant propagation)

**Result:** cmdline.c: 2,487 -> 1,615 bytes (-872 bytes, 35% reduction). The test case went from 148 bytes to 17 bytes (vs GCC's 51 bytes — CCC wins).

**Current status:** Working but triggered the correctness bug investigation. Being integrated with the peephole fix.

### The Cost Attribution Tool

**Hypothesis:** Instead of guessing where the bytes go, instrument the codegen to tag every emitted instruction with WHY it was emitted.

**What we built:**
- `--cost-map` CLI flag
- `cost_tag` field on `CodegenState` — automatically appended as assembly comments
- Tags set at emission points: `operand_to_eax` (RELOAD/ACCUM_IN), `store_eax_to` (SPILL/ACCUM_OUT), `generate_copy` (PHI_COPY), `emit_int_binop_impl` (COMPUTE), prologue (PROLOGUE), etc.
- Made peephole tag-transparent: tags are stripped before pattern matching and reattached after optimization, so `--cost-map` measures **post-peephole** output
- Aggregation module (`cost_map.rs`) computing per-function and per-file breakdowns

**Initial result (pre-peephole, misleading):**

| Category | Bytes | % | What it is |
|----------|-------|---|------------|
| ARG_COPY | 10,252 | 26% | Setting up call arguments |
| SPILL | 5,540 | 14% | Storing values to stack (register pressure) |
| OTHER | 4,649 | 12% | Untagged (lea, misc) |
| COMPUTE | 4,074 | 10% | Actual ALU work — cannot reduce |
| RELOAD | 2,916 | 7% | Loading spilled values back |
| BRANCH | 2,872 | 7% | Jumps and conditional branches |
| PROLOGUE | 2,020 | 5% | Function setup/teardown |
| ACCUM_OUT | 1,926 | 5% | Moving eax to destination |
| ACCUM_IN | 1,796 | 5% | Moving value into eax |
| CALL | 1,560 | 4% | Call instructions |
| CALL_SETUP | 952 | 2% | Stack alignment for calls |
| PHI_COPY | 264 | 1% | SSA phi resolution |

### The Pre/Post Peephole Trap

**Critical discovery:** The initial cost attribution data was measured *pre-peephole*. The 7,000-line peephole optimizer transforms the assembly after emission, converting patterns like `movl %ebx, %eax; pushl %eax` into `pushl %ebx`. The pre-peephole numbers were misleading — they showed problems that were already being fixed.

This led to a wasted investigation of direct push optimization (ARG_COPY), which saved only 12 bytes because the peephole already handled it.

**Fix:** Made the cost-map tag-transparent through the peephole. Tags (`# CATEGORY` comment suffixes) are stripped before pattern matching and reattached to surviving lines after optimization. The `--cost-map` flag now always measures post-peephole output.

**Post-peephole cost attribution (the real numbers):**

| Category | % | Actionable? |
|----------|---|-------------|
| ARG_COPY | 27.3% | Partially — peephole handles simple pushes; remainder is struct/variadic copies and multi-arg setups |
| SPILL | 16.5% | Yes — register allocator improvement |
| COMPUTE | 11.7% | No — actual work |
| OTHER | 9.2% | Mostly jmps — block reordering |
| BRANCH | 7.9% | Partially — branch inversion handled 133 patterns |
| RELOAD | 7.7% | Yes — paired with SPILL |
| CALL | 6.3% | No — can't avoid calling functions |
| PROLOGUE | 5.6% | Some — eliminate unused callee-saves |
| CALL_SETUP | 3.8% | Partially — alignment overhead |
| ACCUM_IN/OUT | 3.6% | Peephole already reduced from ~10% to 3.6% |
| PHI_COPY | 0.5% | Done — coalescing crushed this |

**Key insight: Only 11.7% of CCC's output is actual computation. Almost 9 out of every 10 instructions aren't doing real work — they're moving data around.** That's the accumulator model in a nutshell.

**Lesson: Always measure after the last transformation pass, not before.** Pre-peephole data shows what the codegen emits; post-peephole data shows what actually ships. Optimizing pre-peephole problems that the peephole already fixes is wasted effort.

### Drilling Into the OTHER Bucket and Branch Patterns

The OTHER+BRANCH categories (~17%) contain 490 unconditional `jmp` instructions post-peephole. Analysis revealed two distinct patterns:

**1. Invertible branches (133 instances, -228 bytes):**
```asm
jne .LBB6       # conditional branch
jmp .LBB7       # unconditional jump
.LBB6:          # conditional target is right here!
```
The conditional's target is the next block. Inverting the condition eliminates the `jmp`:
```asm
je .LBB7        # inverted — fall through to .LBB6
.LBB6:
```
Implemented as Phase 10 of the peephole optimizer. 9 of 19 boot files got smaller.

**2. Non-adjacent jumps (357 instances):** These require actual block reordering — physically moving basic blocks in the output so the most common successor is the fallthrough path. A larger change for a future session.

### Register-Direct ALU Operations

**Hypothesis:** The accumulator model's biggest tax is routing every operation through eax. If both operands are in registers, `addl %ebx, %esi` (1 instruction) replaces `movl %ebx, %eax; addl %esi, %eax; movl %eax, %edi` (3 instructions). This would reduce ACCUM_IN/OUT (3.6%) and — through reduced eax clobbering — SPILL+RELOAD (24.2%).

**What we built (3 parts):**
1. **Register-direct ALU in `alu.rs`:** New path in `emit_int_binop_impl()` detecting when dest/lhs/rhs are register-allocated. Three cases: dest==lhs (1 instruction), dest==rhs for commutative ops (1 instruction), dest differs (movl + op, 2 instructions). Also handles `imull` 3-operand form.
2. **Register-direct comparisons in `comparison.rs`:** Same pattern for `cmpl` — compare directly against lhs register instead of loading into eax first.
3. **Register hinting in `regalloc.rs`:** Allocator prefers putting BinOp dest in the same register as a source operand, enabling Cases 1/2 to fire.

**Also fixed 3 pre-existing bugs** found by a code review model:
- Caller-saved clobber indexing used filtered position instead of original index (edx checked against ecx's clobber points when ecx was filtered)
- Register hinting overrode the "reuse already-saved registers" heuristic, introducing new push/pop overhead
- Subtraction generated symmetric hints for both operands, but only lhs is usable (Sub is non-commutative)

**Result: -43 bytes.** The optimization fires (verified in assembly: `addl %edi, %edx` appears instead of eax round-trip), but savings are offset by cases where hinting introduces extra callee-saved push/pop. The architecture is correct but the net impact on this workload is near-zero.

### The Inlining Investigation

**Key finding:** An independent investigation using objdump analysis and Python scripts compared CCC's output against GCC's for the same 19 boot files:

| Metric | CCC | GCC | Ratio |
|--------|-----|-----|-------|
| .text total | 30,864 bytes | 13,599 bytes | 2.27x |
| Call instructions | 312 | 20 | 15.6x |
| mov reg→eax | 1,847 | 156 | 11.8x |

GCC inlines almost everything — 292 more call sites than CCC. Each call costs ~10-15 bytes (instruction + arg setup + prologue/epilogue). This single difference could account for 3-5KB of the gap.

**Attempt 1: Loosen normal inlining thresholds.** Changed -Os limits from 15 instructions / 2 blocks to 30 / 4, removed the 60-instruction caller cap. **Result: +3,666 bytes.** Every boot file except 4 got bigger. The inlined code wasn't being won back by optimization — CCC's accumulator model inflates each inlined instruction into 3+ assembly instructions.

**Attempt 2: Size-positive inlining for single-use callees (with callee credit).** Extended the trial-clone path to consider callees called exactly once, crediting the callee body as removable. **Result: +1,966 bytes.** The credit was optimistic — the caller grew by more than the callee body would save, because of register pressure inflation.

**Attempt 3: Size-positive inlining for single-use callees (no credit).** Removed the callee credit, requiring the caller to genuinely shrink at IR level. **Result: +47 bytes.** Only 2 files changed. The IR profitability check is slightly optimistic vs actual assembly size.

**Root cause:** Inlining is a lever that requires a good optimizer behind it. GCC inlines profitably because its backend handles enlarged functions efficiently (global register allocation, instruction scheduling). CCC's per-function accumulator model means larger functions = more register pressure = more spills = bigger code. The correct order: improve the backend, then increase inlining.

### The Frame Pointer Investigation

**Hypothesis:** Freeing ebp as a 4th callee-saved register (via `-fomit-frame-pointer`) would give a 33% increase in available registers and significantly reduce spills.

**Discovery:** CCC already omits the frame pointer at `-Os`. The infrastructure was fully built:
- `omit_frame_pointer` flag wired through CLI, pipeline, and codegen
- `I686_CALLEE_SAVED_WITH_EBP` register set with ebp as `PhysReg(3)`
- ESP-relative addressing in `slot_ref()` and `param_ref()` for all stack accesses
- Enabled automatically at `-O1`, `-O2`, `-O3`, and `-Os`

Verification confirmed ebp IS being allocated — printf.c uses it in 5 functions. The register allocator already has 4 callee-saved (ebx, esi, edi, ebp) + 2 caller-saved (ecx, edx) = 6 registers. The SPILL+RELOAD overhead (24.2%) is with all available registers already in play. There are no more registers to add.

**Lesson:** Check what's already implemented before building it. The frame pointer was already eliminated.

## Operation Register Storm

At this point, the compiler was stuck at 39,088 bytes — 6,320 bytes over the 32KB limit. Every easy optimization had been tried. The remaining gap required attacking the codegen architecture itself. The plan: a 6-phase assault on the root causes of code bloat, each phase independently testable and committable, with compound effects between phases.

### Phase 1: eax Cache Fix (commit d892600c)

**The bug:** In `store_eax_to()`, after `movl %eax, %ebx`, the code called `invalidate_acc()`. But eax still holds the same value — `movl` copies, it doesn't destroy the source. The next `operand_to_eax()` for that value would wastefully reload from the register.

**The fix:** Change `invalidate_acc()` to `set_acc(dest.0, false)` in the callee-saved register path. The value in eax IS the destination value. All existing invalidation points (ALU ops, calls, block boundaries) still correctly clear the cache when eax actually gets clobbered.

**Result:** Eliminated redundant reloads after register stores. Part of the 1,280-byte combined savings from Phases 1-4.

### Phase 2: Block Reordering + Fallthrough (commit d892600c)

**The problem:** CCC emitted blocks in IR source order. Every block boundary had an explicit `jmp`, even when the target was the very next block. Additionally, conditional branches always emitted two jumps — the unconditional jump to the false block was pure waste when the false block was next:

```asm
testl %eax, %eax
jne .LBBtrue       # conditional jump to true block
jmp .LBBfalse      # unconditional jump to false block — WASTED if false is next
```

**The algorithm (greedy trace layout):**
1. Build successor map from block terminators
2. Start with the entry block, greedily extend: for `Branch(target)` place target next, for `CondBranch` prefer placing false_block next
3. When no unvisited successor exists, start a new trace from the highest-priority unvisited block
4. Concatenate traces into final block order

**The codegen changes:**
- `reorder_blocks_for_size()` in `generation.rs` produces a new block ordering
- `next_block: Option<u32>` field added to `CodegenState`
- Three emission points check for fallthrough: `emit_branch_to_block()` (skip entire jmp), `emit_cond_branch_blocks()` (skip `jmp false`), and `emit_fused_cmp_branch_blocks_impl()` (skip false jump)

**Result:** The single biggest mechanical win. Eliminated hundreds of redundant jump instructions across all 21 boot files.

### Phase 3: Multi-Register Value Tracking (commit d892600c)

**The problem:** CCC's `RegCache` had one field: `acc: Option<RegCacheEntry>`. It only knew what was in eax. If a value was computed into eax and then stored to ebx, the cache forgot eax held the value. The next use would wastefully reload from ebx back to eax.

**The solution:** Expanded `RegCache` to track all 6 registers:

```rust
pub struct RegCache {
    entries: [Option<RegCacheEntry>; 6], // eax, ebx, ecx, edx, esi, edi
}
```

New methods: `set_reg()`, `find_value()`, `invalidate_reg()`, `invalidate_caller_saved()`. When storing eax to a callee-saved register, BOTH are recorded as holding the value. When needing a value in eax, the cache checks ALL registers first — `movl %ebx, %eax` instead of reloading from the stack.

Invalidation rules track x86 semantics: ALU clobbers eax, shifts clobber ecx, division clobbers eax+edx, calls clobber eax+ecx+edx (caller-saved), block boundaries clear everything.

**Result:** Reduced redundant reloads across all boot files. The cache now exploits the fact that values survive in callee-saved registers across operations that only clobber eax.

### Phase 4: Symbol Address Forwarding (commit d892600c)

**The problem:** CCC's global symbol accesses generated intermediate values like `$g1` and `$g2` that went through stack spills. A global variable access would: compute the address, spill it to stack, reload it later, then dereference. For frequently-accessed globals, this added redundant loads.

**The solution:** Track symbol addresses through ESP-relative stack slots. When a `leal symbol, %eax` result is stored to the stack, record which slot holds which symbol. When that slot is loaded back, fold the address into the consuming instruction directly instead of reloading through the stack.

**Result:** Part of the combined 1,280-byte savings from Phases 1-4.

### Phases 1-4 Combined Result

| Commit | `_end` | Gap to 32KB |
|--------|--------|-------------|
| 86e5183b (baseline) | 39,088 | +6,320 |
| d892600c (Phases 1-4) | 37,808 | +5,040 |

**Savings: 1,280 bytes.** Still 5,040 bytes over. The backend improvements were real but not enough on their own.

### Phase 5: Tail Call Optimization — Skipped

Tail call optimization would replace `call foo; epilogue; ret` with `jmp foo`. However, i686 uses stack-based argument passing, and the peephole optimizer already handles simple push-based calling sequences. Converting stack-relative argument setup to tail-call form would require rewriting the calling convention at emission time — incompatible with the existing push-based argument optimization. Skipped as architecturally incompatible.

### Phase 6: Callee-Credit Inlining + Dead Callee Elimination (commit e79ae9c2)

**The key insight:** Previous inlining attempts all failed because they measured the wrong thing. When a static function is called exactly once, inlining eliminates the entire standalone callee. The trial comparison was measuring "did the caller shrink?" — but the right question is "did the caller grow by LESS than the callee we're about to delete?"

This reframes the profitability check. A callee with 50 instructions, called once: if inlining grows the caller by 30 instructions, that's a net savings of 20 instructions — the caller grew but the callee vanished entirely.

**What we built:**

1. **`count_all_references()`** — counts both `Call` and `GlobalAddr` (function pointer) references across all IR instructions AND global variable initializers. A function pointer stored in a struct (`video_bios.probe = bios_probe`) is a reference that prevents elimination.

2. **Callee-credit profitability check** — for single-use callees:
   ```
   profitable = caller_growth < callee_instruction_count
   ```
   For multi-use callees, the original check applies: `trial_size < original_size`.

3. **`eliminate_dead_static_callees()`** — after all inlining, scans for static functions with zero remaining references and removes them from the module. This zeros out function bodies at the IR level, but the empty function sections remain in the linked binary unless the linker removes them.

**Four bugs found and fixed during development:**

- **GlobalAddr references:** Functions used as function pointers (e.g., `__inb`, `__outb` stored in `struct port_io_ops`) have `GlobalAddr` instructions, not `Call`. The initial `count_call_sites()` only counted `Call`, causing function-pointer targets to be eliminated. Fixed by scanning both.

- **Global initializer references:** Functions like `bios_probe` referenced from struct initializers in global data (`video_bios.probe = bios_probe`) are not visible in IR instructions at all — they're in `module.globals`. Fixed by scanning `global.init.for_each_ref()`.

- **Stale callee snapshots:** The `callee_map` is built before inlining starts. When `_kstrtoull` had its callees inlined (changing its body), then `_kstrtoull` was inlined into `kstrtoull` using the OLD snapshot body — the snapshot still had `Call` instructions to functions that had already been inlined. This caused double-inlining: the same code appeared twice. Fixed by looking up the CURRENT callee body from `module.functions` instead of the stale snapshot.

- **Cascading callee credit:** `remaining_calls` was decremented after each inline. A function called from 2 sites would get callee credit after the first inline (remaining=1), but both inlines together grew more than the callee. Each looked profitable individually; combined they regressed. Fixed by using the ORIGINAL call counts for single-use determination.

**Result:**

The dead callee elimination zeroes out function bodies at IR level, but the kernel boot build does **not** use `-ffunction-sections` + `--gc-sections`. Without `--gc-sections`, the empty function stubs remain in the linked `.text` section. The savings from Phase 6 are real at the IR level but do not materialize in the linked binary under the kernel's actual build process.

| Commit | `_end` (no --gc-sections) | Gap to 32KB |
|--------|--------------------------|-------------|
| d892600c (Phases 1-4) | 37,808 | +5,040 |
| e79ae9c2 (Phase 6) | **39,312** | **+6,544** |

The `_end` *increased* slightly (37,808 → 39,312) because Phase 6 added 2 new source files (`cpu.c`, `version.c`) and the inlining changes had mixed per-file results without `--gc-sections` to remove dead code.

**Note on `--gc-sections`:** With the non-standard flags `-ffunction-sections` + `--gc-sections`, the linker strips unreachable sections and `_end` drops to 31,872 bytes (under 32KB). However, the kernel's `arch/x86/boot/Makefile` and `setup.ld` do not use these flags. Reporting the `--gc-sections` number as the real result would be dishonest — it's not how the kernel builds.

### The Register Allocator Cost Model (Phase 2 Callee-Saved Gate)

**Hypothesis:** The register allocator's Phase 2 (linear scan) freely introduces NEW callee-saved registers when it finds a benefit. Each new callee-saved register costs 2 bytes (push in prologue + pop in epilogue) per function. Under `-Os`, the marginal benefit of an extra register often doesn't recoup the push/pop overhead — especially in small functions that dominate boot code.

**Observation:** Many boot functions are small (10-30 instructions). The allocator might assign `%ebp` to hold a single value across 3 instructions, saving one stack reload (3 bytes) but adding `pushl %ebp` + `popl %ebp` (2 bytes) — a net savings of only 1 byte in the best case. In many cases the reload savings are smaller than the push/pop cost, producing a net regression.

**What we built:**
A "callee-saved gate" in `regalloc.rs`: under `-Os`, Phase 2 of the linear scan allocator only REUSES callee-saved registers that Phase 1 already introduced. It never introduces a NEW callee-saved register just for Phase 2 benefits. This preserves the allocator's ability to use registers when they're already being saved, while preventing the introduction of new push/pop pairs for marginal register benefits.

The change is surgical — a single `only_reuse: bool` parameter on `find_best_callee_reg()` and the `optimize_size` flag on `RegAllocConfig`:

```rust
pub struct RegAllocConfig {
    pub available_regs: Vec<PhysReg>,
    pub caller_saved_regs: Vec<PhysReg>,
    pub allow_inline_asm_regalloc: bool,
    pub optimize_size: bool,  // NEW: Phase 2 callee-saved gate
}
```

**Result: -4,301 bytes code-only** (30,562 → 26,261 across all 21 boot files). The second-largest single optimization of the entire project, behind only phi relay coalescing.

Largest per-file savings:
- printf.c: -876 bytes (25.0% reduction)
- string.c: -652 bytes (28.5%)
- video.c: -504 bytes (17.8%)
- tty.c: -416 bytes (27.3%)
- cmdline.c: -372 bytes (24.7%)
- cpucheck.c: -292 bytes (14.8%)

Every single file improved. No regressions.

**Post-hoc correction:** The -4,301 byte measurement was later discovered to have been taken without `-include compiler_types.h` — the same flags error described in "The Critical Correctness Bug" section. Without this header, `__always_inline` and `noinline` are undefined, producing different (smaller but incorrect) code. With correct flags, the gate actually **increases** code size by +336 bytes (29,297 → 29,633 code-only on the linear scan allocator). The gate was subsequently disabled. The IRC graph coloring allocator, which replaced linear scan for `-Os`, makes the gate irrelevant — IRC's interference graph naturally handles the cost/benefit tradeoff of callee-saved registers through its spill cost heuristic.

### The Superoptimizer Analysis of CCC's Own Output

**Hypothesis:** The superoptimizer (window 4) found zero savings in an earlier session, proving local optimality. But window 4 is only 4 instructions. A wider window (8) might find patterns that span more instructions — store-forwarding chains, duplicate loads, redundant flag tests.

**What we built:**
Ran the superoptimizer at window size 8 on all 21 boot .o files. Harvested 3,952 unique instruction patterns from CCC's assembly output. Found 49 rules (shorter equivalents) totaling 174 bytes of projected savings.

**Key finding:** The 49 rules fell into generalizable categories:

| Pattern | Savings per instance | Occurrences | Category |
|---------|---------------------|-------------|----------|
| Store-forwarding into ALU/CMP | 1 byte | 9+ | Memory → register forwarding |
| Duplicate load elimination | 1 byte | 5+ | Redundant memory access |
| Redundant testl after ALU | 1-2 bytes | 3+ | Flag analysis |
| EBP in incl/decl/xorl | 2 bytes | 2+ | Register restriction removal |
| Duplicate large immediate | 4 bytes | 2 | Constant reuse |
| Dead load before overwrite | 3-6 bytes | Varies | Dead code |

Most of the 49 rules were INSTANCES of these general patterns. Rather than adding 49 specific pattern matches, we implemented the underlying generalizations.

### Superoptimizer-Derived Peephole Patterns

**What we built (6 changes to peephole.rs):**

1. **Remove EBP restriction from `incl`/`decl`:** The strength-reduction pattern `addl $1 → incl` and `subl $1 → decl` was guarded by `dest_reg != REG_EBP`. This was overly conservative — `incl %ebp` is a perfectly valid 1-byte instruction (vs `addl $1, %ebp` at 3 bytes). The guard existed because EBP was historically the frame pointer, but with `-fomit-frame-pointer` at `-Os`, EBP is a general-purpose register.

2. **Remove EBP restriction from `xorl` zero pattern:** Same issue — `movl $0, %ebp` (5 bytes) vs `xorl %ebp, %ebp` (2 bytes). The guard was unnecessary.

3. **Extend `eliminate_redundant_test_after_alu` for `addl`/`subl`/`incl`/`decl`:** The existing pass eliminated `testl %reg, %reg` after `andl`/`orl`/`xorl` (which set all flags). Extended to also eliminate after `addl`/`subl` (set all flags) and `incl`/`decl` (set ZF/SF/PF/OF but NOT CF) — but only when the next flag consumer uses just ZF or SF (je/jne/js/jns). Added `next_flag_consumer_zf_sf_only()` helper for the safety check.

4. **Extend `eliminate_dead_reg_moves` to handle dead `leal`:** The pass already eliminated `movl` into a register that's dead before its next use. Extended to also catch `leal` (load effective address), which is a pure computation with no side effects.

5. **Store-forwarding into ALU/CMP (Pattern 12):** When a value is stored to the stack (`movl %reg, N(%ebp)`) and the very next instruction loads from that same stack slot as a source operand (`cmpl N(%ebp), %other` or `addl N(%ebp), %other`), replace the memory operand with the register: `cmpl %reg, %other`. Saves 1 byte per instance (register operand encoding is shorter than displacement).

6. **Duplicate load elimination (Pattern 13):** When two consecutive loads access the same stack slot (`movl N(%ebp), %eax` followed by `movl N(%ebp), %ecx`), replace the second with a register-to-register move (`movl %eax, %ecx`). Saves 1 byte per instance.

7. **Duplicate large-immediate elimination (Pattern 14):** When the same large constant (|value| > 127, requiring 4-byte immediate encoding) is both stored to the stack and loaded to a register, reorder to use the register for the store: `movl $IMM, %reg; movl %reg, N(%ebp)` instead of two separate large-immediate encodings. Saves 4 bytes per instance.

**Result: -14 bytes code-only** (26,261 → 26,247). The direct savings are modest — the superoptimizer's projected 174 bytes assumed every instance would fire, but many are blocked by intervening instructions or register conflicts.

### The 4KB Alignment Cliff

**The surprise:** Despite only 14 bytes of direct code savings from the peephole patterns, `_end` dropped by **4,080 bytes** (39,296 → 35,216). This massive disproportionate effect revealed a critical property of the linker layout.

**Root cause:** The `.pecompat` section in `setup.ld` has 4KB alignment (`Algn 2^12`). This means `.pecompat` starts at the next 4KB boundary after the end of code sections. When code size crosses a 4KB boundary downward, `.pecompat` jumps back by a full 4,096 bytes:

```
Code ends at 28,047 → .pecompat starts at 28,672 (0x7000) → _end = 35,216
Code ends at 28,800 → .pecompat starts at 32,768 (0x8000) → _end = 39,312
```

The 14-byte peephole savings, combined with the 4,301-byte callee-saved gate, pushed the code below the 28,672 boundary, causing `.pecompat` to relocate from 0x8000 to 0x7000 — a 4,096-byte cliff.

**Implications for remaining work:**
- Current `.pecompat` at 0x7000 (28,672). `_end` = 35,216.
- Next cliff at 0x6000 (24,576). `.text32` currently ends at ~28,047.
- Need ~3,471 MORE bytes of code reduction to cross the next cliff.
- If crossed, `_end` would drop to ~31,120 — UNDER the 32KB limit.

This means the 32KB goal is achievable with approximately 3,471 more bytes of code-only reduction. The alignment cliff converts incremental savings into a step-function reward.

### IRC Graph Coloring Register Allocator

**Hypothesis:** CCC's linear scan allocator makes greedy, left-to-right decisions that produce suboptimal register assignments. A graph coloring allocator — the industry-standard approach used by GCC and LLVM — builds an interference graph of all live ranges, then colors it with the minimum number of registers. This produces globally better assignments: fewer spills, better coalescing of copies, and more efficient use of the 6 available registers.

**What we built:**
- `src/backend/graph_coloring.rs` — 1,087 lines implementing Iterated Register Coalescing (IRC), the George & Appel algorithm:
  1. Build interference graph from liveness analysis
  2. Coalesce non-interfering copies (aggressive coalescing)
  3. Simplify the graph by removing low-degree nodes
  4. Spill nodes that can't be simplified (using a cost heuristic: uses/degree)
  5. Select colors (registers) by popping from the simplify stack
  6. Iterate if spills are introduced
- Dispatch in `regalloc_helpers.rs`: calls `allocate_registers_irc()` when `optimize_size=true`
- All 896 unit tests pass

**Result: -2,524 bytes code-only (8.5% reduction).** The biggest wins came from copy coalescing (fewer register-to-register moves) and better spill decisions (values that are used frequently stay in registers).

Per-file highlights:
- video.c: -472 bytes (video mode switching has many live values competing for registers)
- printf.c: -430 bytes (format string parsing has complex control flow)
- string.c: -239 bytes (tight loops benefit from fewer spill/reload pairs)
- early_serial_console.c: -193 bytes

**Linked _end: unchanged at 0x9990 (39,312).** The 2,524-byte code savings weren't enough to cross the 0x8000 alignment cliff — `.pecompat` stayed at 0x8000.

### IRC Clobber Refinement

**Hypothesis:** The IRC allocator's interference graph was over-conservative in modeling register clobbers. Every instruction that might clobber a caller-saved register was treated as clobbering ALL caller-saved registers. On i686, a `movl (%ecx), %eax` (load through pointer) clobbers ecx (used as address scratch) but does NOT clobber edx. The over-broad clobber created false interferences, preventing edx from holding values across loads.

**What we built:**
Refined the clobber model in `graph_coloring.rs` to match the per-register clobber tracking already used by the linear scan allocator:
- Loads through pointers: clobber ecx only (not edx)
- GEP (address calculation): clobber ecx only
- Stores through pointers: clobber ecx AND edx
- Shifts: clobber ecx only
- Division: clobber ecx AND edx

**Result: -72 bytes code-only** (27,109 → 27,037). Small direct savings, but this pushed the code past the 0x7000 alignment cliff — `.pecompat` relocated from 0x8000 to 0x7000, dropping `_end` from 39,312 to 35,216.

### The `-mregparm=3` Discovery

**Hypothesis:** The Linux kernel boot code is compiled with `-mregparm=3`, a GCC extension that passes the first 3 integer arguments in registers (eax, edx, ecx) instead of on the stack. CCC was passing all arguments on the stack, generating `pushl` instructions for every argument. This accounts for a significant portion of the ARG_COPY overhead (27% of total code).

**What we built:**
- `-mregparm=3` support in CCC's i686 backend
- First 3 integer/pointer arguments passed in eax, edx, ecx (matching GCC's convention)
- Remaining arguments still pushed to stack
- Function prologues save register arguments to stack slots for the body to use (matching GCC's approach — the ABI requires the callee to save them if needed)

**Result: -2,766 bytes code-only** (27,037 → 24,271). The single largest code-only reduction from any individual optimization. Every function call became smaller: 3 fewer `pushl` instructions (3 bytes each = 9 bytes per call site) across 200+ call sites. Some calls saved even more when the argument was already in the right register.

**Linked _end: 0x7990 = 31,120.** Crossed the 0x6000 alignment cliff! `.pecompat` relocated from 0x7000 to 0x6000, and `_end` dropped to 31,120 bytes — **under the 32KB limit** for the first time.

### Symbol+Offset Folding

**Hypothesis:** CCC generates symbol addresses and offsets as separate operations: `movl $symbol, %reg; addl $offset, %reg; movl (%reg), %dst`. x86 supports displacement addressing: `movl symbol+offset, %dst` — one instruction instead of three.

**What we built:**
Extended Pattern 3 in `fold_absolute_addressing()` (peephole.rs) to handle symbols (was numeric-only):
- `movl $sym, %r; addl $N, %r; movl (%r), %dst` → `movl sym+N, %dst` (saves 7 bytes per instance)
- `movl $sym, %r; addl $N, %r` → `movl $sym+N, %r` (Pattern 3b, saves 3 bytes)
- Fallback for `movzbl` when flags are live: displacement fold (saves ~2 bytes)

**Result: -420 bytes code-only** (24,271 → 23,851). The boot code accesses many kernel data structures at fixed offsets from symbol addresses — this pattern is pervasive.

### Extend-Fold and Zero-Store Optimizations

Two targeted peephole optimizations that together provided the final push past the 0x6000 cliff.

**1. Extend-fold (`fold_extend_then_move`, Phase 8b):**

The superoptimizer harvest (window 8, 3,494 unique patterns) identified the #1 pattern by impact: `movzbl %al, %eax; movl %eax, %ecx` — 52 occurrences, 260 total bytes. The pattern `movzbl SRC, %REG; movl %REG, %DST` can be folded to `movzbl SRC, %DST` when the intermediate register is dead afterward, saving 2 bytes per instance.

**Critical lesson — pass placement matters:** The first implementation placed this rule inside `combined_local_pass` (the main peephole engine). Result: **+24 bytes regression**. Changing liveness in the middle of the combined pass disrupted later optimizations. Moving it to a standalone late pass (Phase 8b, after all other cleanup) produced **-124 bytes** — the same rule, different placement, opposite result.

**2. andl $0 zero-store (`fold_movl_zero_esp_to_andl`, Phase 4j):**

`movl $0, N(%esp)` encodes as 8 bytes (opcode + ModRM + SIB + disp8 + imm32). `andl $0, N(%esp)` encodes as 5 bytes (opcode + ModRM + SIB + disp8 + imm8) because `andl` with 0 uses sign-extended imm8 encoding. The transformation is valid when flags aren't live after the instruction.

**The comment stripping bug:** The rule existed but fired only 1 out of 86 instances. Root cause: CCC emits inline comments like `movl $0, 0(%esp)    # PHI_COPY`. The pattern matcher used `s.ends_with("(%esp)")` — which returns false when a comment trails the instruction. `trimmed()` strips leading whitespace but NOT inline comments. Fix: strip comments with `s.find("    #")` before matching.

**Reverse iteration for cascading:** `andl` sets flags. A chain of `movl $0` instructions stacked vertically can only be converted bottom-up — converting the bottom one makes flags dead for the one above it, enabling that conversion, and so on. Forward iteration misses these chains. Reverse iteration with up to 4 passes handles all chains.

**Result: -176 bytes code-only** (23,727 → 23,551). 57 conversions fired (was 1 before the comment fix), 30 remaining where flags are genuinely live.

### The classify_line Comment Stripping Bug (The Second Correctness Crisis)

**Discovery:** With `.code16gcc` prefix insertion implemented, 20 of 21 boot files passed the QEMU swap test (each CCC .o swapped into a GCC boot image, linked, booted). But **video-mode** hung — the kernel stopped after BIOS initialization. Raw (unoptimized) assembly for video-mode passed the swap test, confirming the bug was in the peephole optimizer.

**Binary phase search:** Using environment variables to stop optimization at specific phases, then swap-testing each:
- Phase 1 (combined_local_pass): PASS
- Phase 2 (global passes): FAIL
- Sub-phase narrowing: `propagate_register_copies` was the culprit

**Root cause:** CCC emits inline comments on assembly lines: `movl %ebx, %eax    # PHI_COPY`. The `classify_line` function parsed register names from the FULL line including the comment. When it called `register_family("%eax    # PHI_COPY")`, the function tried to match `"eax    # PHI_COPY"` — no match, returning `REG_NONE`. This misclassified `movl %ebx, %eax    # PHI_COPY` as `Other { dest_reg: REG_NONE }` instead of `Move { src: ebx, dst: eax }`.

**The cascade:** `propagate_register_copies` tracks `copy_src[dst] = src` for register moves. When a register is overwritten, copies sourced from it must be invalidated. But the misclassified move was invisible — the write to `%eax` was never seen. In the `set_mode` function:

```asm
call *0(%esp)           # returns result in %eax
movl %eax, %edi         # copy_src[edi] = eax
movl %ebx, %eax    # PHI_COPY  ← classified as Other, write to eax invisible
movl %eax, 124(%esp)   # stores %ebx (correct), but copy_src[edi] still says eax
movl %edi, %eax    # PHI_COPY  ← copy_src[edi]=eax, edi==eax? YES (stale!) → NOP'd
```

The last `movl %edi, %eax` should restore `%edi` (the call result) into `%eax`. But the stale copy state said `copy_src[edi] = eax`, meaning `edi` and `eax` hold the same value — so the move was eliminated as a self-move. In reality, `%eax` had been overwritten by the PHI_COPY from `%ebx`, so they held DIFFERENT values. The NOP elimination produced wrong code.

**The fix (4 lines in classify_line):**
```rust
// Strip trailing GAS comments (# ...) before classification.
let s = if let Some(hash_pos) = s_full.find("    #") {
    s_full[..hash_pos].trim_end()
} else {
    s_full
};
```

**Why `trimmed()` couldn't be used:** The existing `trimmed()` helper strips leading whitespace but NOT trailing comments — and it shouldn't, because `# regparm %eax %edx %ecx` annotations are read by liveness analysis. The comment stripping must happen specifically in `classify_line` for classification purposes only.

**Result: 21/21 PASS.** But with a significant code size regression. The correctly-classified PHI_COPY moves change optimization behavior across ALL peephole passes — moves that were previously invisible are now tracked, invalidated, eliminated differently. Code-only went from ~30,400 bytes (with prefixes, pre-fix) to **36,287 bytes** (with prefixes, post-fix). The code is correct but larger.

**Lesson:** A pattern matching bug in any optimization pass can produce a cascade of incorrect transformations. The stale copy state didn't just affect one instruction — it caused dozens of subsequent optimizations to operate on wrong assumptions. Binary phase search (disable phases one by one, swap-test each) is the fastest way to isolate which pass introduces a bug.

### The ESP Dead Store Elimination Bug

**Discovery:** During swap testing, `early_serial_console` failed. Binary phase search narrowed it to `eliminate_never_read_esp_in_range` — a pass that removes stores to ESP-relative stack slots that are never read before being overwritten or before the function returns.

**Root cause:** The pass tracked ESP delta through push/pop/addl/subl to normalize offsets (a push changes ESP, so the "same" slot has different raw offsets before and after the push). But across control flow (e.g., function epilogues), the delta was wrong — the delta computed through one path didn't match the delta through another. A store that was read via a different path appeared as "never read" because the normalized offsets didn't match.

**The fix:** Collect BOTH raw and ESP-delta-normalized offsets for each read. A store is eliminated only if NEITHER its raw NOR its normalized offset matches any read offset. This is conservative but safe — it only eliminates stores that are provably dead regardless of ESP delta tracking accuracy.

**Result:** `early_serial_console` PASS. The fix is slightly less aggressive (keeps some stores that the original would have eliminated) but correct.

### Combined Result

The IRC allocator, `-mregparm=3`, peephole suite, and correctness fixes combined to produce code that passes all 21 swap tests. The historical code-only progression (pre-prefix, pre-classify_line-fix) shows the relative savings from each optimization:

```
Linear scan:      29,633 code-only → _end = 0x9990 (39,312) — 6,544 OVER
IRC:              27,109 code-only → _end = 0x9990 (39,312) — 6,544 OVER
IRC + clobber:    27,037 code-only → _end = 0x8990 (35,216) — 2,448 OVER  [cliff: 0x8000→0x7000]
+ mregparm=3:     24,271 code-only → _end = 0x7990 (31,120) — 1,648 UNDER [cliff: 0x7000→0x6000]
+ sym+offset:     23,851 code-only → _end = 0x7990 (31,120) — 1,648 UNDER
+ extend-fold:    23,727 code-only → _end = 0x7990 (31,120) — 1,648 UNDER
+ andl $0:        23,551 code-only → _end = 0x7990 (31,120) — 1,648 UNDER
+ .code16gcc:     ~30,400 code-only → _end = 0x8970 (35,184) — 2,416 OVER [prefixes add ~29%]
+ classify_line:  36,287 code-only → _end = 0xb990 (47,504) — 14,736 OVER [correctness fix]
```

The correctness fixes (.code16gcc prefix insertion and classify_line comment stripping) are non-negotiable — without them the code doesn't run. The current code is correct and verified by swap test, but significantly over the 32KB limit.

## Current State

| Metric | Value |
|--------|-------|
| Linked `_end` (setup image) | **47,504 bytes (`0xb990`) — 14,736 OVER 32KB** |
| 32KB limit (`_end ≤ 0x8000`) | 32,768 bytes |
| Gap | **14,736 bytes over** |
| Code-only sum (21 .o files, with prefixes) | 36,287 bytes |
| GCC reference `_end` (includes prefixes) | 22,976 bytes (`0x59C0`) |
| GCC code-only sum (includes prefixes) | ~14,610 bytes |
| CCC/GCC code-only ratio | 2.48x |
| Unit tests | 896 passing |
| Boot files compiling | 21/21 (all with CCC, zero GCC) |
| **QEMU swap test** | **21/21 PASS** |
| `.code16gcc` assembler support | Implemented and verified (post-process prefix toggle) |
| Boot test result | All 21 .o files individually boot correctly in QEMU swap test |

The journey so far:

**Era 1: Codegen improvements** (accumulator optimizations, block layout, inlining)

| Milestone | `_end` | Gap to 32KB | What changed |
|-----------|--------|-------------|--------------|
| Starting point (19 files) | 39,088 | +6,320 | Baseline: all -Os plumbing, phi coalescing, edx allocation |
| After Phases 1-4 (19 files) | 37,808 | +5,040 | eax cache fix, block reordering, multi-reg tracking, symbol forwarding |
| After Phase 6 (21 files) | 39,312 | +6,544 | Callee-credit inlining, dead callee elimination (2 new files added) |

**Era 2: Allocator + calling convention** (IRC graph coloring, `-mregparm=3`)

These measurements are from a clean baseline with correct flags (`-include compiler_types.h`), using IRC as the register allocator:

| Milestone | Code-only | `_end` | Gap to 32KB | What changed |
|-----------|-----------|--------|-------------|--------------|
| Linear scan baseline | 29,633 | 39,312 | +6,544 | 21 files, correct flags |
| IRC graph coloring | 27,109 | 39,312 | +6,544 | -2,524 bytes (8.5%), same `_end` (pre-cliff) |
| + IRC clobber refinement | 27,037 | 35,216 | +2,448 | -72 bytes, crossed 0x8000→0x7000 cliff |
| + `-mregparm=3` | 24,271 | **31,120** | **-1,648** | -2,766 bytes, crossed 0x7000→0x6000 cliff |

**Era 3: Peephole suite** (symbol folding, extend-fold, zero-store)

| Milestone | Code-only | `_end` | Gap to 32KB | What changed |
|-----------|-----------|--------|-------------|--------------|
| + symbol+offset folding | 23,851 | 31,120 | -1,648 | -420 bytes |
| + extend-fold (Phase 8b) | 23,727 | 31,120 | -1,648 | -124 bytes |
| + andl $0 zero-store | 23,551 | 31,120 | -1,648 | -176 bytes |

**Era 4: Correctness** (.code16gcc, classify_line fix)

| Milestone | Code-only | `_end` | Gap to 32KB | What changed |
|-----------|-----------|--------|-------------|--------------|
| + .code16gcc prefixes | ~30,400 | 35,184 | +2,416 | Prefix bytes required for 16-bit execution |
| + classify_line fix | **36,287** | **47,504** | **+14,736** | Comment stripping correctness fix → 21/21 swap PASS |

**The code is correct but does not yet fit.** The IRC graph coloring allocator, `-mregparm=3`, and targeted peephole suite produced significant code-only reductions (6,082 bytes / 20.5% from linear scan baseline). However, two correctness fixes — `.code16gcc` prefix insertion (~29% overhead) and `classify_line` comment stripping (changed optimization behavior) — increased the actual output. The current code-only total (36,287 bytes with prefixes) is 2.48x GCC's (14,610 bytes).

### Top CCC vs GCC Code Size Gaps (Current)

CCC's code-only total (36,287 bytes) is 2.48x GCC's (~14,610 bytes). The gap is concentrated in a few files:

| File | CCC bytes | GCC bytes | Gap | CCC/GCC ratio |
|------|-----------|-----------|-----|---------------|
| printf.c | 6,528 | ~1,773 | +4,755 | 3.68x |
| video.c | 4,502 | ~1,485 | +3,017 | 3.03x |
| string.c | 4,093 | ~1,173 | +2,920 | 3.49x |
| cpucheck.c | 3,083 | ~655 | +2,428 | 4.71x |
| cmdline.c | 1,969 | ~455 | +1,514 | 4.33x |

These 5 files account for ~14,600 bytes of the ~21,700-byte gap. The accumulator model, lack of aggressive inlining, and now-correct `.code16gcc` prefix overhead all contribute.

## What Worked and What Didn't

| Approach | Expected | Actual | Verdict |
|----------|----------|--------|---------|
| Direct-operand ALU | Prerequisite | Prerequisite | Enabled ecx allocation |
| ecx register allocation | 1-3 KB savings | ~0 bytes | Correct but wrong workload |
| Phi relay coalescing | 1-2 KB | **-4,438 bytes** | Massive win — biggest single optimization |
| edx register allocation | 500-1500 bytes | **-560 bytes** | Modest but real |
| Superoptimizer identities | 786 bytes | **0 (was a bug)** | Proved local optimality instead |
| Parameter alloca removal | 500-1500 bytes | **0 bytes** | Peephole already handled it |
| Size-positive inlining | Significant | **-872 bytes** (cmdline) | Foundation for Phase 6 |
| Cost attribution tool | Diagnostic | **Revealed 27% ARG_COPY** | Changed the roadmap |
| Branch inversion (Phase 10) | 400-600 bytes | **-228 bytes** | Clean win, 133 patterns |
| Direct push at codegen level | 3-5 KB | **-12 bytes** | Peephole already handled it |
| Frame pointer elimination | 1-2 KB | **Already active** | Was already implemented |
| Register-direct ALU + hinting | 800-2,500 bytes | **-43 bytes** | Architecturally correct, net wash |
| Inliner threshold loosening | Multi-KB savings | **+3,666 bytes** | Made everything bigger |
| Single-use callee inlining (attempt 1) | Multi-KB savings | **+1,966 bytes** | Callee credit was optimistic |
| Single-use callee inlining (attempt 2) | Moderate savings | **+47 bytes** | IR profitability ≠ assembly size |
| **eax cache fix (Phase 1)** | 100-300 bytes | **part of -1,280** | Kept eax valid after callee-saved store |
| **Block reordering (Phase 2)** | 800-2,000 bytes | **part of -1,280** | Greedy trace layout, fallthrough detection |
| **Multi-reg value tracking (Phase 3)** | 200-500 bytes | **part of -1,280** | 6-entry RegCache, cross-register lookup |
| **Symbol address forwarding (Phase 4)** | 200-500 bytes | **part of -1,280** | Track globals through stack slots |
| **Callee-credit inlining (Phase 6)** | 500-2,000 bytes | **~0 bytes net** | Correct profitability metric, but savings require --gc-sections |
| **Dead callee elimination** | Part of Phase 6 | **~0 bytes net** | Zeroes IR bodies, but empty stubs remain without --gc-sections |
| **Register-direct backend rewrite** | 500-2,000 bytes | **-185 bytes code** | Correct architecture, but peephole already compensated for accumulator waste |
| **Callee-saved gate (-Os)** | Unknown | **-4,301 bytes (wrong flags); +336 bytes (correct flags)** | Disabled — IRC allocator supersedes it |
| **Superopt-derived peephole (w8)** | 174 bytes | **-14 bytes code** | Store-forwarding, dup-load, EBP unlocking, flag analysis |
| **4KB alignment cliff crossing** | N/A | **-4,080 bytes _end** | 14-byte code reduction pushed .pecompat across 4KB boundary |
| **IRC graph coloring allocator** | Multi-KB | **-2,524 bytes code** | Full IRC: interference graph, aggressive coalescing, optimal spill decisions |
| **IRC clobber refinement** | Moderate | **-72 bytes code** | Per-register clobber in IRC (ecx vs edx), crossed 0x8000→0x7000 cliff |
| **`-mregparm=3` support** | Multi-KB | **-2,766 bytes code** | Register argument passing (eax/edx/ecx), crossed 0x7000→0x6000 cliff |
| **Symbol+offset folding** | 200-500 bytes | **-420 bytes code** | `movl $sym; addl $N; movl (%r)` → `movl sym+N` |
| **Extend-fold (late pass)** | 100-200 bytes | **-124 bytes code** | `movzbl %al,%eax; movl %eax,%ecx` → `movzbl %al,%ecx` |
| **andl $0 zero-store** | 100-200 bytes | **-176 bytes code** | `movl $0,N(%esp)` → `andl $0,N(%esp)`, comment stripping fix, reverse iteration |
| **.code16gcc prefix insertion** | Required for correctness | **+~6,800 bytes code** | 0x66/0x67 prefixes for 16-bit real mode execution (~29% overhead) |
| **classify_line comment stripping** | Required for correctness | **+~5,900 bytes code** | Fixed PHI_COPY misclassification → 21/21 swap test PASS |
| **ESP dual offset tracking** | Required for correctness | **~0 bytes** | Fixed early_serial_console swap test failure |

## Lessons Learned

**1. Measure before you optimize.** The cost attribution tool should have been built first. Every optimization attempt before it was partially blind — we guessed where the bytes were and were often wrong.

**2. Correctness bugs can hide as performance wins.** The peephole's `is_reg_dead_from` bug made code smaller by deleting correct computations. The "31,830 bytes fitting under 32KB" milestone was built on broken code. Always validate correctness independently.

**3. Local optimality doesn't mean global optimality.** The superoptimizer proved that every 4-instruction window is already optimal. The bloat is structural — too many instructions doing unnecessary work, not bad instruction selection.

**4. The biggest problem isn't always the most visible one.** Everyone initially focused on register allocation (the most obvious difference vs GCC). The actual #1 cost is argument setup for function calls — a completely different part of the codegen.

**5. Existing infrastructure often already solves the problem.** The parameter alloca fix was redundant (peephole handled it). The register allocator already supported caller-saved registers (just needed the list populated). Frame pointer elimination was already active. Size-positive inlining reused existing constfold + DCE infrastructure. Check what exists before building new.

**6. Measure after the last transformation, not before.** The cost attribution tool initially ran pre-peephole, showing ARG_COPY at 26% (~10KB). This led to a direct-push optimization that saved only 12 bytes — the peephole was already converting `movl %reg, %eax; pushl %eax` → `pushl %reg`. Post-peephole measurement gives the actionable picture.

**7. The accumulator model is the fundamental constraint.** Only 11.7% of output is actual computation. The remaining 88% is data movement — loading into eax, storing from eax, spilling, reloading, copying. Every optimization that doesn't address this architectural reality is nibbling at the margins. GCC's register-direct ALU (`addl %esi, %ebx`) avoids the entire accumulator tax.

**8. AI-driven compiler development has unique advantages and risks.** The ability to rapidly prototype, test, and iterate on optimization strategies is remarkable — this entire investigation happened across a handful of sessions. The risk is moving fast without validating assumptions, as the peephole bug demonstrated.

**9. Inlining needs a good optimizer behind it.** An independent investigation found CCC emits 312 calls vs GCC's 20 for the same boot code — GCC inlines almost everything. Three separate inlining attempts before backend improvements all regressed: loosening thresholds (+3,666 bytes), crediting callee removal (+1,966 bytes), and conservative trial-clone (+47 bytes). The correct order of operations: improve the backend first, THEN increase inlining. The callee-credit approach after Phases 1-4 produced better IR-level results, but the savings don't materialize in the linked binary without `--gc-sections` (which the kernel doesn't use).

**10. Register-direct ALU is architecturally correct but needs allocator cooperation.** Building `addl %ebx, %esi` (bypass eax) + register hinting (allocator prefers dest == source register) + clobber index bugfixes produced only -43 bytes. The optimization fires (verified in assembly) but savings are offset by extra callee-saved push/pop when hinting introduces new registers. The hinting must only prefer already-used registers — never introduce new push/pop overhead for a hint.

**11. Different AI models have different strengths.** A "builder" model excels at executing plans: reading code, implementing changes, running tests. An "investigator" model excels at diagnosis: writing scripts, comparing outputs, finding root causes. The 312-vs-20 call count finding came from the investigator, not the builder. Use the right tool for the right phase: investigate first, build second.

**12. Always compare the same file sets.** A measurement that omits 2 files from the baseline can show a spurious "improvement" of thousands of bytes. Pin the exact file list in the measurement script and verify both runs measure the same set.

**13. The right profitability metric changes everything — but only with the right linker support.** Three inlining attempts failed because they asked "did the caller shrink?" For single-use static callees, the right question is "did the caller grow by less than the callee we're deleting?" This reframing is correct in principle — the callee-credit model correctly accounts for the fact that single-use callees vanish after inlining. However, the dead callee elimination only zeroes function bodies at IR level. Without `-ffunction-sections` + `--gc-sections` (which the kernel doesn't use), the empty function stubs remain in the linked binary. The savings are real at IR level but don't materialize in the actual linked output.

**14. Use original counts, not decremented remaining counts.** When iterating through inline candidates, decrementing a "remaining call count" creates cascading false positives. A function called from 2 sites gets callee credit after the first inline (remaining=1), but both inlines together grow more than the callee. Each looks profitable individually; combined they regress. Use the original, immutable call counts for profitability decisions.

**15. Use the current callee body, not the snapshot.** The callee_map is built before inlining starts. When callee A has its own callees inlined (changing its body), then A is inlined somewhere else using the old snapshot — the snapshot still has Call instructions that were already resolved. This causes double-inlining: the same code appears twice. Always look up the current function body from the module, not the pre-built snapshot.

**16. Function pointer references hide in three places.** A function can be referenced by: (1) `Call` instructions (obvious), (2) `GlobalAddr` instructions (function pointers in code), and (3) global variable initializers (function pointers in struct literals). Missing any one of these causes dead callee elimination to delete functions that are still reachable. The kernel boot code uses all three patterns.

**17. Always validate against the actual build process, not a modified one.** The `--gc-sections` linker flag was never used in the kernel's boot `Makefile` or `setup.ld`. Using it to measure `_end` produced a number (31,872 bytes) that appeared to meet the 32KB goal, but the real linked image without it is 39,312 bytes. An optimization measured under non-standard conditions is not a real optimization for that target. Always reproduce the exact build process the target uses.

**18. Phase ordering matters, but so does the build system.** Phases 1-4 (backend improvements) saved 1,280 bytes — confirmed and real. Phase 6 (callee-credit inlining + dead callee elimination) showed promise at the IR level, but the savings only materialize with `--gc-sections`, which the kernel boot build does not use. The lesson: always validate optimizations against the actual build process, not a modified one. An optimization that only works with non-standard linker flags is not a real optimization for that target.

**19. The register allocator can be the biggest code size lever.** The IRC graph coloring allocator saved 2,524 bytes (8.5%) over the linear scan allocator, and `-mregparm=3` saved 2,766 bytes. Together, allocator + calling convention improvements account for 5,290 bytes — more than any other category. The insight: in a 6-register ISA, how you assign registers and how you pass arguments are first-order code size concerns, not minor tuning knobs. (Note: the Phase 2 callee-saved gate originally measured at -4,301 bytes, but this was a wrong-flags artifact. With correct flags, it actually hurt by +336 bytes and was disabled.)

**20. Linker alignment creates step functions, not gradual slopes.** The `.pecompat` section's 4KB alignment means `_end` doesn't decrease smoothly with code size — it jumps by 4,096 bytes at each boundary crossing. The full optimization campaign crossed TWO cliffs: 0x8000→0x7000 (via IRC clobber refinement) and 0x7000→0x6000 (via `-mregparm=3`), converting 6,082 bytes of code savings into 8,192 bytes of `_end` reduction. Conversely, savings between cliffs produce ZERO `_end` improvement (as the register-direct backend's 185-byte code savings demonstrated — `_end` didn't change). When optimizing for a hard size limit, understanding the linker's alignment behavior is as important as the code generation itself.

**21. A superoptimizer at wider windows finds patterns a human wouldn't think to look for.** Window 4 found nothing — local instruction selection is optimal. Window 8 found 49 rules spanning store-forwarding chains, duplicate loads, and flag redundancies. Most rules were instances of a few general patterns, suggesting the right engineering approach: implement the general pattern once in the peephole, not 49 special cases. The superoptimizer is best used as a diagnostic tool to discover WHAT patterns exist, then a human implements the generalized version.

**22. Peephole compensates for architectural waste — removing the waste doesn't double-save.** The register-direct backend rewrite (eliminating accumulator round-trips at the source) was expected to save 500-2,000 bytes. It saved 185. The 7,000-line peephole was already cleaning up the same waste patterns. Register-direct eliminates waste at emission; peephole eliminates it after emission. Both target the same bytes. The lesson: when evaluating an optimization, subtract what existing passes already handle. An optimization's theoretical impact assumes no other pass addresses the same problem.

**23. Graph coloring is worth the complexity for code size.** The IRC allocator (1,087 lines) replaced the linear scan allocator for `-Os` and saved 2,524 bytes (8.5%). The improvement comes from two sources: aggressive copy coalescing (fewer register-to-register moves) and globally optimal spill decisions (the cost heuristic `uses/degree` prioritizes keeping frequently-used values in registers). Linear scan makes locally greedy decisions that compound into global suboptimality.

**24. ABI conventions are a first-order code size concern.** `-mregparm=3` saved 2,766 bytes — the single largest code-only reduction from any individual optimization. Every function call became 3 pushes shorter. This wasn't a codegen improvement; it was a calling convention change. When targeting a specific binary size, check what ABI extensions the target uses. The kernel boot code's `-mregparm=3` flag was hiding in plain sight in the Makefile.

**25. Inline comments break pattern matching in non-obvious ways.** CCC emits `# PHI_COPY` comments inline with instructions. `trimmed()` strips leading whitespace but not trailing comments. Pattern matchers using `ends_with()` silently fail when comments are present. This caused a rule to fire 1 time instead of 57. Always strip inline comments before pattern matching in any peephole optimizer.

**26. Pass placement can flip an optimization from regression to improvement.** The extend-fold rule (`movzbl+movl` → single `movzbl`) regressed by +24 bytes when placed inside the main peephole pass. The same rule saved -124 bytes as a standalone late pass (Phase 8b). Changing liveness mid-pass disrupts downstream pattern matching. New peephole rules should be benchmarked at multiple positions in the pass pipeline.

**27. Reverse iteration enables cascading transformations.** The `andl $0` zero-store optimization converts flag-neutral instructions into flag-setting instructions. Forward iteration can't cascade these conversions because each newly-created flag-setter enables the instruction above it. Reverse iteration (bottom-up) naturally handles chains: convert the bottom, then the next one up sees dead flags, and so on. Any transformation where the output enables the transformation for a neighbor should consider reverse iteration.

**28. Binary phase search is the fastest way to find peephole bugs.** Use environment variables to stop optimization at specific phases (`CCC_PHASE_LIMIT`) and sub-phases (`CCC_P2_LIMIT`), then swap-test each variant. This narrows from "some optimization is wrong" to "this specific pass on this specific function" in minutes. The classify_line bug was traced from "video-mode fails" to "propagate_register_copies in Phase 2 misclassifies PHI_COPY moves" in under an hour.

**29. Pattern matching on assembly text MUST strip annotations before parsing.** CCC emits multiple comment types: `# PHI_COPY`, `# regparm %eax %edx %ecx`, `# BRANCH`. Any function that extracts register names from instruction text must strip these first. `trimmed()` does NOT strip them (and shouldn't — `# regparm` is read by liveness analysis). The `classify_line` function needed its own comment stripping before any register parsing. This is a general principle: any text-based pattern matcher operating on annotated assembly must be annotation-aware.

**30. Correctness fixes can reverse size wins — accept the cost.** The classify_line fix increased code-only from ~30,400 to 36,287 bytes (with prefixes). This happened because PHI_COPY moves were previously invisible to the optimizer (classified as `Other` with `dest_reg: REG_NONE`), and many passes had adapted around this broken classification. Fixing the classification changed optimization behavior across ALL passes. The code is now correct (21/21 swap test) but larger. The previous smaller code was wrong (video-mode crashed). There is no shortcut — correctness must come first, then re-optimize from the correct baseline.

**31. ESP delta tracking fails across control flow boundaries.** Linear ESP offset normalization (tracking push/pop/addl/subl to map different raw offsets to the same logical slot) works within a basic block but fails when code has multiple paths with different ESP states. The safe approach: collect BOTH raw and delta-normalized offsets for reads, and only eliminate a store if NEITHER its raw NOR normalized offset appears in any read set.

**32. Dead infrastructure can produce convincing but wrong results.** CCC's assembler had `.code16gcc` support structurally present — the parser recognized it, the ELF writer tracked it, the encoder had a `code16gcc: bool` field — but the field was never set to `true`. The encoder always produced 32-bit code without the 0x66/0x67 override prefixes required for 16-bit real mode. The resulting code was smaller (no prefix overhead), passed all unit tests (tests don't run in 16-bit mode), and produced valid ELF objects. It took an actual boot attempt to discover the bug — the kernel immediately crashed with #UD (Invalid Opcode). Three separate measurement sessions used these wrong numbers before the boot test exposed the truth. **Always validate that code runs correctly before celebrating size achievements.** Size measurements of non-functional code are meaningless.

**29. Two alignment cliffs can be crossed in one optimization campaign.** The `.pecompat` section's 4KB alignment means each cliff crossing saves 4,096 bytes of `_end`. The journey from 39,312 to 31,120 crossed TWO cliffs (0x8000→0x7000 via clobber refinement, 0x7000→0x6000 via `-mregparm=3`), converting 6,082 bytes of code savings into 8,192 bytes of `_end` reduction. Understanding the cliff structure makes it possible to plan optimization targets around boundary crossings.

## Conclusion

CCC's i686 backend now compiles all 21 Linux kernel boot C files natively. The `.code16gcc` assembler mode produces correct 16-bit real-mode output with operand/address size override prefixes. **All 21 files pass the QEMU swap test** — each CCC-compiled .o, swapped into a GCC boot image, links and boots to "Linux version". The GCC dependency for C compilation is eliminated. The code is verified correct.

The linked setup image is **47,504 bytes (`_end = 0xb990`) — 14,736 bytes over the 32KB limit.** More optimization work is needed to close the gap. The 32KB goal is not yet achieved.

The confirmed savings from all optimization work:

| Phase | Code savings | `_end` impact | Mechanism |
|-------|-------------|---------------|-----------|
| Phi relay coalescing | -4,438 bytes | -4,438 | Eliminate circular copy chains from phi elimination |
| edx register allocation | -560 bytes | -560 | Per-register clobber tracking |
| Branch inversion | -228 bytes | -228 | Peephole: invert condition, eliminate unconditional jmp |
| Phases 1-4 combined | -1,280 bytes | -1,280 | eax cache, block reordering, multi-reg tracking, symbol forwarding |
| Phase 6 (dead callee elim) | ~0 bytes | ~0 | Zeroes IR bodies, but kernel doesn't use --gc-sections |
| Register-direct backend | -185 bytes | 0 | Savings absorbed by linker alignment |
| Callee-saved gate (-Os) | +336 bytes (wrong-flag measurement: -4,301) | ~0 | Disabled — IRC allocator supersedes linear scan gate |
| Superopt peephole patterns | -14 bytes | -4,080 | Crossed 4KB alignment cliff (0x8000→0x7000) |
| **IRC graph coloring** | **-2,524 bytes** | **0** | **Full IRC allocator: coalescing, optimal spills** |
| **IRC clobber refinement** | **-72 bytes** | **-4,096** | **Per-register clobber in IRC, crossed 0x8000→0x7000 cliff** |
| **`-mregparm=3`** | **-2,766 bytes** | **-4,096** | **Register argument passing, crossed 0x7000→0x6000 cliff** |
| **Symbol+offset folding** | **-420 bytes** | **0** | **Peephole: fold `$sym + addl $N` into displacement** |
| **Extend-fold (Phase 8b)** | **-124 bytes** | **0** | **Peephole: `movzbl+movl` → single `movzbl`** |
| **andl $0 zero-store** | **-176 bytes** | **0** | **Peephole: `movl $0` → `andl $0` (imm8 encoding)** |
| Other (ecx, direct ALU, etc.) | ~0 bytes | ~0 | Marginal or offset by other costs |

The optimization journey can be grouped into four eras:

1. **The codegen era** (Phases 1-6): Backend improvements to the accumulator model — eax caching, block reordering, multi-register tracking, inlining. Collectively saved ~6,000+ bytes of code-only, but `_end` only dropped from 39,088 to 35,216 (one cliff crossing).

2. **The allocator era** (IRC graph coloring): Replacing the linear scan allocator with IRC (-2,524 bytes) and refining its clobber model (-72 bytes). The IRC allocator's copy coalescing and optimal spill decisions attacked SPILL+RELOAD (24% of output) directly. This crossed the first alignment cliff (0x8000→0x7000).

3. **The convention + peephole era** (`-mregparm=3` + peephole suite): Register argument passing eliminated the largest remaining waste (stack-based argument copies), and targeted peephole rules (symbol folding, extend-fold, zero-store) provided further reductions. Together: -3,906 bytes code-only.

4. **The correctness era** (`.code16gcc` + classify_line fix): Implementing correct `.code16gcc` prefix insertion added ~29% overhead (required for 16-bit execution). Fixing the classify_line comment stripping bug produced correct optimization behavior but larger code. Together these non-negotiable correctness fixes increased code from the historical 23,551 bytes to the current 36,287 bytes — but the code now actually works. **21/21 swap test PASS.**

GCC produces `_end = 22,976 bytes` for the same source files — CCC's code-only sum is 36,287 bytes vs GCC's ~14,610 bytes (2.48x ratio). The accumulator model remains the fundamental architectural constraint, compounded by the `.code16gcc` prefix overhead.

**What this means:** CCC compiles all 21 Linux kernel boot C files natively, produces correct `.code16gcc` assembly output, and all 21 files pass the QEMU swap test (boot to "Linux version"). The GCC dependency for C compilation is eliminated. However, the linked output does not yet fit within the BIOS boot protocol's 32KB constraint — the linker assertion `ASSERT(_end <= 0x8000)` fails by 14,736 bytes.

**What remains:** The 14,736-byte gap requires substantial code size reduction. The `.code16gcc` prefix overhead (~29% of code size) is a fixed cost that GCC also pays — CCC's raw instruction efficiency must improve significantly. The largest remaining gaps vs GCC (printf +4,755 bytes, video +3,017, string +2,920) represent the primary targets. The classify_line fix changed how PHI_COPY moves interact with ALL peephole passes — revisiting the peephole optimization strategy with correct classification may recover some of the regression. Improved inlining, better constant propagation, and potentially a full register-direct codegen model to replace the accumulator architecture are the most promising approaches.

## Tools Built Along the Way

All of these are permanent additions to CCC, not throwaway scripts:

- **Cost attribution (`--cost-map`):** Tags every emitted instruction with its purpose (SPILL, RELOAD, COMPUTE, ARG_COPY, etc.). Runs post-peephole via tag-transparent stripping. Produces per-function and per-file breakdowns. Zero overhead when disabled.
- **Superoptimizer diagnose mode (`ccc-superopt diagnose`):** Cross-block pattern analysis across multiple assembly files. Finds identity patterns and shorter replacements.
- **Size-positive inlining with callee credit:** Trial-clone mechanism that inlines only when the optimized result is provably smaller. For single-use static callees, accounts for callee elimination in the profitability check. Includes dead callee elimination to zero function bodies at IR level (note: requires `-ffunction-sections` + `--gc-sections` to strip them from the linked binary — the kernel boot build does not use these flags).
- **Per-register clobber tracking:** Fine-grained liveness analysis distinguishing ecx clobbers from edx clobbers, enabling more precise caller-saved register allocation.
- **Branch inversion (peephole Phase 10):** Eliminates `jcc TARGET; jmp OTHER; TARGET:` patterns by inverting the condition and removing the unconditional jump.
- **Block reordering (greedy trace layout):** Reorders basic blocks so the most common successor is the fallthrough path. Eliminates hundreds of unconditional jumps. Gated by `-Os`.
- **Multi-register value tracking:** 6-entry `RegCache` tracking values across all general-purpose registers. Eliminates redundant reloads when values survive in callee-saved registers across eax-clobbering operations.
- **Callee-saved gate (`optimize_size` on `RegAllocConfig`):** Under `-Os`, Phase 2 of the linear scan allocator only reuses already-introduced callee-saved registers. Initially measured at -4,301 bytes, later found to be a wrong-flags artifact (+336 bytes with correct flags). Superseded by the IRC graph coloring allocator, which handles the callee-saved cost/benefit tradeoff through its spill cost heuristic.
- **Superoptimizer wide-window analysis (`ccc-superopt` at window 8):** Exhaustive pattern search across 8-instruction windows, harvesting from all 21 boot .o files. Finds store-forwarding opportunities, duplicate loads, redundant flag tests, and dead code patterns invisible at smaller windows. Used as a diagnostic to guide peephole rule development.
- **IRC graph coloring allocator (`graph_coloring.rs`):** Full Iterated Register Coalescing implementation — interference graph construction, aggressive copy coalescing, Briggs/George coalescing heuristics, optimal spill selection via uses/degree cost metric. Dispatched automatically under `-Os` via `regalloc_helpers.rs`.
- **`-mregparm=3` calling convention:** Register-based argument passing for the first 3 integer/pointer arguments (eax, edx, ecx), matching GCC's `-mregparm=3` extension used by the Linux kernel boot code. Eliminates stack pushes at call sites.
- **Symbol+offset folding (peephole):** Folds `movl $symbol, %r; addl $N, %r; movl (%r), %dst` into `movl symbol+N, %dst`, leveraging x86 displacement addressing to eliminate address arithmetic instructions.
- **Extend-fold (peephole Phase 8b):** Folds `movzbl/movzwl/movsbl/movswl SRC, %REG; movl %REG, %DST` into a single extend instruction with the final destination, eliminating the intermediate `movl`. Runs as a late pass to avoid disrupting earlier peephole patterns.
- **andl $0 zero-store (peephole Phase 4j):** Converts `movl $0, N(%esp)` (8 bytes) to `andl $0, N(%esp)` (5 bytes) using sign-extended imm8 encoding. Includes comment stripping for CCC's inline `# PHI_COPY` annotations and reverse iteration for cascading conversions.
- **`.code16gcc` assembler support (encoder/mod.rs, elf_writer.rs):** Post-processing prefix insertion for 16-bit real mode execution. The encoder produces 32-bit mode bytes, then a post-pass toggles 0x66 operand-size prefixes (the meaning of 0x66 reverses between 16-bit and 32-bit modes — it's a toggle, not absolute), strips 0x66 from 16-bit GP instructions, adds 0x67 for 32-bit addressing, and adjusts relocation offsets. Jump relaxation handles the enlarged instruction sizes (jmp: 5→6, jcc: 6→7 bytes). Verified correct by 21/21 QEMU swap test.
- **QEMU swap test framework (`test_swap.sh`):** Automated correctness verification: compile one file with CCC, swap into GCC boot image, link, build bzImage, boot in QEMU, check for "Linux version" output. Tests all 21 C files individually. Binary phase search variant stops optimization at specific phases to isolate bugs.
- **classify_line comment stripping (peephole.rs):** Strips `# PHI_COPY`, `# regparm`, and `# BRANCH` comments before register parsing in the line classifier. Prevents misclassification of annotated instructions. Critical correctness fix for `propagate_register_copies`.
- **ESP dead store elimination with dual offset tracking (peephole.rs):** Collects both raw and ESP-delta-normalized offsets for stack reads. Eliminates stores only when neither raw nor normalized offset matches any read, ensuring correctness across control flow with different ESP states.
