# Reproducing Boot Code Size Measurements

## Overview

CCC compiles the Linux kernel's 16-bit real-mode boot code (21 C files from
`arch/x86/boot/`). The goal is to fit the linked image under the 32KB limit
(`_end ≤ 0x8000`). This document describes exactly how to reproduce the
measurements.

**Current results (2026-02-10):**

| Config | Code-only (21 files) | Linked _end | Swap test | Status |
|--------|---------------------|-------------|-----------|--------|
| CCC -Os -mregparm=3, full peephole (WITH prefixes) | 36,287 bytes | **47,504 (0xb990)** | **21/21 PASS** | OVER by 14,736 |
| GCC -Os -mregparm=3 (reference, includes prefixes) | ~14,610 bytes | 22,976 (0x59C0) | 21/21 PASS | Under |

32KB limit = 32,768 bytes (0x8000).

**_end = 0xb990 = 47,504 bytes — 14,736 bytes OVER the 32KB limit. All 21 files pass the QEMU swap test.**

### Key correctness milestones

- **21/21 compile**: All boot C files compile with CCC natively (no GCC)
- **21/21 swap test PASS**: Each CCC-compiled .o swapped into GCC boot image, linked,
  booted in QEMU — prints "Linux version" for all 21 files
- **.code16gcc prefix insertion**: 0x66/0x67 prefixes correctly inserted for 16-bit
  real mode execution (required for boot — without them, #UD Invalid Opcode)

### Correction history

> **Correction (2026-02-10):** Previous measurements reported code-only as 23,551 bytes
> and _end as 31,120 (under 32KB). Those numbers were WRONG for two reasons:
>
> 1. **Missing .code16gcc prefixes (2026-02-09):** The assembler's `code16gcc` field was
>    never wired up. Code was encoded as plain 32-bit — no 0x66/0x67 prefixes. Smaller
>    but couldn't execute in 16-bit mode (#UD on boot). With prefixes: _end = 0x8970 = 35,184.
>
> 2. **classify_line comment stripping bug (2026-02-10):** `classify_line` parsed register
>    names from text including `# PHI_COPY` comments. `register_family("%eax    # PHI_COPY")`
>    returned REG_NONE, misclassifying moves as `Other`. This broke `propagate_register_copies`
>    (writes invisible, stale copies persisted → incorrect NOP elimination). Fixing this
>    produced correct but larger code. With both fixes: code-only = 36,287, _end = 0xb990 = 47,504.

### Optimization journey

Code-only measurements (WITH .code16gcc prefixes, correct classify_line):

| Stage | Code-only | Linked _end | Delta |
|-------|-----------|-------------|-------|
| Current (all optimizations + correctness fixes) | **36,287** | **47,504 (0xb990)** | baseline |
| GCC reference | ~14,610 | 22,976 (0x59C0) | target |

The following historical measurements were taken WITHOUT .code16gcc prefixes and
WITHOUT the classify_line fix. They document the relative savings from each
optimization, which are still valid, but the absolute numbers are not comparable
to the current (correct) numbers:

| Stage (historical, pre-prefix, pre-fix) | Code-only | Linked _end | Delta |
|----------------------------------------|-----------|-------------|-------|
| Linear scan allocator | 29,633 | 39,312 (0x9990) | baseline |
| + IRC graph coloring | 27,109 | 39,312 (0x9990) | -2,524 |
| + IRC clobber refinement | 27,037 | 35,216 (0x8990) | -72 |
| + -mregparm=3 | 24,271 | 35,216 (0x8990) | -2,766 |
| + symbol+offset folding | 23,851 | 35,216 (0x8990) | -420 |
| + extend-fold (movzbl+movl) | 23,727 | 35,216 (0x8990) | -124 |
| + andl $0 zero-store | 23,551 | 31,120 (0x7990) | -176 |

## Prerequisites

- **x86-64 Linux box** (Azure VM, bare metal, or WSL2) with:
  - Rust toolchain (`rustup`, `cargo`)
  - GNU binutils: `ld`, `as`, `cpp`, `nm`, `size` (the standard `binutils` package)
  - `wget`, `make`, `gcc` (for kernel header generation only)
  - QEMU (`qemu-system-x86_64`) for swap test verification
  - Linux 6.9 kernel source (downloaded below)
- CCC source code (this repository), on the `feat/i686-Os` branch

**NOTE:** CCC cross-compiles i686 code on x86-64. The VM does NOT need to be
32-bit. The GNU assembler (`as --32`) and linker (`ld -m elf_i386`) handle
32-bit output.

### VM setup used for verified measurements

```
Host: 20.114.191.87 (Azure, Ubuntu 22.04)
CCC:  /home/azureuser/ccc
Kernel: /tmp/linux-6.9
```

### Step 0: Install Rust (if needed)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### Step 1: Download and prepare Linux 6.9 kernel source

```bash
cd /tmp
wget https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.9.tar.xz
tar xf linux-6.9.tar.xz
cd linux-6.9
make ARCH=x86 defconfig
make ARCH=x86 headers_install

# Verify these files exist (required for compilation):
ls include/generated/autoconf.h
ls arch/x86/boot/zoffset.h
```

### Step 2: Create boot_compat.h

This header provides kernel attribute macros that CCC supports but that are
normally defined by the kernel's build infrastructure. Without it, attributes
like `__init`, `__packed`, and `likely()`/`unlikely()` are undefined.

```bash
cat > /tmp/boot_compat.h << 'COMPAT'
#ifndef _BOOT_COMPAT_H
#define _BOOT_COMPAT_H
#define __init __attribute__((__section__(".init.text")))
#define __cold __attribute__((__cold__))
#define __noreturn __attribute__((__noreturn__))
#define __packed __attribute__((__packed__))
#define __weak __attribute__((__weak__))
#define __aligned(x) __attribute__((__aligned__(x)))
#define __printf(a,b) __attribute__((format(printf,a,b)))
#define __must_check __attribute__((__warn_unused_result__))
#define likely(x) __builtin_expect(!!(x), 1)
#define unlikely(x) __builtin_expect(!!(x), 0)
#define IS_ENABLED(x) 0
#define IS_BUILTIN(x) 0
#define IS_MODULE(x) 0
#define BUILD_BUG_ON(cond) ((void)sizeof(char[1 - 2*!!(cond)]))
#define BUILD_BUG_ON_ZERO(e) ((int)(sizeof(struct { int:(-!!(e)); })))
#define CONFIG_PAGE_SHIFT 12
#define CONFIG_X86_MINIMUM_CPU_FAMILY 3
#define CONFIG_X86_64 1
#define CONFIG_PHYSICAL_START 0x1000000
#define CONFIG_PHYSICAL_ALIGN 0x200000
#ifndef NORMAL_VGA
#define NORMAL_VGA 0xFFFF
#endif
#endif
COMPAT
```

## Build CCC

```bash
# Clone (or use your existing checkout)
cd /home/azureuser/ccc   # adjust path as needed
git switch feat/i686-Os   # MUST be on this branch for peephole optimizations

cargo build --release     # ~2-3 minutes from clean, ~30s incremental
```

The binary is at `target/release/ccc`.

## Compilation Flags

### CRITICAL: ALL of these flags are required

Copy this block exactly. Do NOT omit any flags or add your own.

```bash
KDIR=/tmp/linux-6.9
CCC=/home/azureuser/ccc/target/release/ccc

IFLAGS="-I $KDIR/arch/x86/include \
        -I $KDIR/arch/x86/include/generated \
        -I $KDIR/arch/x86/include/generated/uapi \
        -I $KDIR/include \
        -I $KDIR/arch/x86/include/uapi \
        -I $KDIR/include/uapi"

DFLAGS="-D__KERNEL__ -D_SETUP -D__EXPORTED_HEADERS__ \
        -DSVGA_MODE=NORMAL_VGA -DDISABLE_BRANCH_PROFILING \
        -D__DISABLE_EXPORTS"

PREAMBLE="-include $KDIR/include/linux/compiler_types.h \
          -include /tmp/boot_compat.h"
```

### WARNING about compiler_types.h

**`-include include/linux/compiler_types.h` is MANDATORY.** Without it,
`__always_inline`, `noinline`, and other critical attributes are undefined.
This produces smaller but **incorrect** code — functions that the kernel
expects to be inlined are not, changing behavior and invalidating measurements.

**How to tell if you forgot it:** your code-only total will be significantly
lower than expected. The code compiles fine but is WRONG.

### WARNING about -mregparm=3

**`-mregparm=3` is part of `REALMODE_CFLAGS` in the kernel's `arch/x86/Makefile`.**
It passes the first 3 integer arguments in registers (eax, edx, ecx) instead of
on the stack. Without it, all arguments go on the stack, producing significantly
larger code. This flag was missing from earlier measurements, inflating code-only numbers.

### WARNING about boot_compat.h

**`-include /tmp/boot_compat.h` is MANDATORY.** Without it, macros like
`likely()`, `unlikely()`, `IS_ENABLED()`, `BUILD_BUG_ON()`, `__packed`, and
`__init` are undefined. Compilation may fail or produce wrong code.

## Step-by-Step Reproduction

### 1. Set the variables (run this FIRST in your shell)

```bash
KDIR=/tmp/linux-6.9
CCC=/home/azureuser/ccc/target/release/ccc

IFLAGS="-I $KDIR/arch/x86/include \
        -I $KDIR/arch/x86/include/generated \
        -I $KDIR/arch/x86/include/generated/uapi \
        -I $KDIR/include \
        -I $KDIR/arch/x86/include/uapi \
        -I $KDIR/include/uapi"

DFLAGS="-D__KERNEL__ -D_SETUP -D__EXPORTED_HEADERS__ \
        -DSVGA_MODE=NORMAL_VGA -DDISABLE_BRANCH_PROFILING \
        -D__DISABLE_EXPORTS"

PREAMBLE="-include $KDIR/include/linux/compiler_types.h \
          -include /tmp/boot_compat.h"

FILES="a20 apm cmdline cpu cpucheck cpuflags early_serial_console edd main memory pm printf regs string tty version video-bios video-mode video-vesa video-vga video"
```

### 2. Compile all 21 C files with CCC

```bash
cd $KDIR
rm -rf /tmp/boot_measure && mkdir -p /tmp/boot_measure

for src in $FILES; do
    $CCC -m16 -Os -mregparm=3 -c \
        $IFLAGS $DFLAGS $PREAMBLE \
        arch/x86/boot/${src}.c \
        -o /tmp/boot_measure/${src}.o
done
```

All 21 files must compile without errors. If any file fails, check:
- Is `$KDIR` set correctly? Does `/tmp/linux-6.9` exist?
- Did you run `make ARCH=x86 defconfig && make ARCH=x86 headers_install`?
- Does `/tmp/boot_compat.h` exist?

### 3. Measure code-only per file

```bash
TOTAL=0
for src in $FILES; do
    CODE=$(size -A /tmp/boot_measure/${src}.o \
           | awk '/text/{sum+=$2} END{print sum+0}')
    printf "%-28s %5d\n" "$src" "$CODE"
    TOTAL=$((TOTAL + CODE))
done
echo "TOTAL                        $TOTAL"
```

**Measurement note:** Use `awk '/text/'` (not `awk '/^\.text/'`) to capture
both `.text` and `.inittext` sections. Functions with `__init` attribute
go to `.inittext`.

### 4. Assemble the 4 .S files

These are hand-written assembly files from the kernel. They are assembled
with the GNU assembler, not CCC.

```bash
for src in bioscall copy header pmjump; do
    cpp $IFLAGS \
        -D__KERNEL__ -D_SETUP -DSVGA_MODE=NORMAL_VGA \
        -DDISABLE_BRANCH_PROFILING -D__ASSEMBLY__ \
        -include $KDIR/include/generated/autoconf.h \
        arch/x86/boot/${src}.S \
        -o /tmp/boot_measure/${src}.i
    as --32 /tmp/boot_measure/${src}.i \
        -o /tmp/boot_measure/${src}.o
done
```

### 5. Link and measure _end

```bash
# Remove the ASSERT so we can measure even when over 32KB
cp arch/x86/boot/setup.ld /tmp/boot_measure/setup_noassert.ld
sed -i '/ASSERT(_end <= 0x8000/d' /tmp/boot_measure/setup_noassert.ld

ld -m elf_i386 \
    -T /tmp/boot_measure/setup_noassert.ld \
    -o /tmp/boot_measure/setup.elf \
    /tmp/boot_measure/*.o

# Check _end
echo "=== _end ==="
nm /tmp/boot_measure/setup.elf | grep " _end"

# Full section layout
echo "=== Section layout ==="
size -A /tmp/boot_measure/setup.elf
```

**Do NOT use `--gc-sections`.** The kernel boot build does not use it.

### 6. Swap test (correctness verification)

The swap test verifies that each CCC-compiled object file produces a bootable
kernel. For each of the 21 C files:

1. Compile ONE file with CCC, the remaining 20 with GCC
2. Link the mixed image (CCC .o swapped in for GCC .o)
3. Build bzImage
4. Boot in QEMU
5. PASS = prints "Linux version" within 10 seconds

```bash
# This requires the full test_swap.sh script on the VM
# See /tmp/test_swap.sh on the Azure VM for the full script
```

**Current result: 21/21 PASS** — every CCC-compiled .o boots successfully.

### 7. Compare against GCC (optional)

```bash
rm -rf /tmp/boot_gcc && mkdir -p /tmp/boot_gcc

for src in $FILES; do
    gcc -m16 -Os -c \
        -fomit-frame-pointer -march=i386 -mregparm=3 \
        $IFLAGS $DFLAGS \
        -include $KDIR/include/linux/compiler_types.h \
        -include $KDIR/include/generated/autoconf.h \
        arch/x86/boot/${src}.c \
        -o /tmp/boot_gcc/${src}.o
done

# Same measurement loop as step 3 but on /tmp/boot_gcc/
```

## Expected Results

### Full peephole suite (current, default under -Os)

This is what you should get with the `feat/i686-Os` branch:

```
File                         Code-only (with .code16gcc prefixes)
a20                           1043
apm                            564
cmdline                       1969
cpu                           1058
cpucheck                      3083
cpuflags                      1088
early_serial_console          2284
edd                              0
main                          1103
memory                         766
pm                             613
printf                        6528
regs                           138
string                        4093
tty                            839
version                          0
video-bios                    1243
video-mode                    2190
video-vesa                    1540
video-vga                     1643
video                         4502
TOTAL                        36287
```

Linked: **`_end = 0xb990 = 47,504 bytes`** (14,736 bytes over the 32KB limit)

Swap test: **21/21 PASS** (all files boot correctly in QEMU)

### How alignment cliffs work

The `.pecompat` section requires 4KB alignment. Its position depends on how
much code precedes it. The relationship between code size and `_end` is a
step function:

```
.text32 end position    .pecompat position    _end
─────────────────────   ──────────────────    ──────
< 24,576 (0x6000)       0x6000                ~0x7990 = 31,120
24,576 - 28,671         0x7000                ~0x8990 = 35,216
28,672 - 32,767         0x8000                ~0x9990 = 39,312
32,768 - 36,863         0x9000                ~0xa990 = 43,408
36,864 - 40,959         0xA000                ~0xb990 = 47,504
```

Between cliffs, _end does NOT change — savings are absorbed by alignment
padding. Only when code size crosses a 4KB boundary does _end jump.

## One-Shot Script

For convenience, here is a single script that does everything from step 1
through step 5. Copy-paste it into your shell:

```bash
#!/bin/bash
set -e

KDIR=/tmp/linux-6.9
CCC=/home/azureuser/ccc/target/release/ccc   # adjust path to your CCC binary

IFLAGS="-I $KDIR/arch/x86/include \
        -I $KDIR/arch/x86/include/generated \
        -I $KDIR/arch/x86/include/generated/uapi \
        -I $KDIR/include \
        -I $KDIR/arch/x86/include/uapi \
        -I $KDIR/include/uapi"

DFLAGS="-D__KERNEL__ -D_SETUP -D__EXPORTED_HEADERS__ \
        -DSVGA_MODE=NORMAL_VGA -DDISABLE_BRANCH_PROFILING \
        -D__DISABLE_EXPORTS"

PREAMBLE="-include $KDIR/include/linux/compiler_types.h \
          -include /tmp/boot_compat.h"

FILES="a20 apm cmdline cpu cpucheck cpuflags early_serial_console edd main memory pm printf regs string tty version video-bios video-mode video-vesa video-vga video"

cd $KDIR
rm -rf /tmp/boot_measure && mkdir -p /tmp/boot_measure

echo "=== Compiling 21 C files with CCC ==="
FAIL=0
for src in $FILES; do
    if ! $CCC -m16 -Os -mregparm=3 -c \
        $IFLAGS $DFLAGS $PREAMBLE \
        arch/x86/boot/${src}.c \
        -o /tmp/boot_measure/${src}.o 2>/dev/null; then
        echo "FAILED: ${src}.c"
        FAIL=1
    fi
done
if [ "$FAIL" -eq 1 ]; then echo "Some files failed to compile!"; exit 1; fi
echo "All 21 files compiled successfully."
echo ""

echo "=== Code-only per file ==="
TOTAL=0
for src in $FILES; do
    CODE=$(size -A /tmp/boot_measure/${src}.o | awk '/text/{sum+=$2} END{print sum+0}')
    printf "%-28s %5d\n" "$src" "$CODE"
    TOTAL=$((TOTAL + CODE))
done
echo "TOTAL                        $TOTAL"
echo ""

echo "=== Assembling 4 .S files ==="
for src in bioscall copy header pmjump; do
    cpp $IFLAGS \
        -D__KERNEL__ -D_SETUP -DSVGA_MODE=NORMAL_VGA \
        -DDISABLE_BRANCH_PROFILING -D__ASSEMBLY__ \
        -include $KDIR/include/generated/autoconf.h \
        arch/x86/boot/${src}.S \
        -o /tmp/boot_measure/${src}.i
    as --32 /tmp/boot_measure/${src}.i \
        -o /tmp/boot_measure/${src}.o
done
echo "Done."
echo ""

echo "=== Linking ==="
cp arch/x86/boot/setup.ld /tmp/boot_measure/setup_noassert.ld
sed -i '/ASSERT(_end <= 0x8000/d' /tmp/boot_measure/setup_noassert.ld

ld -m elf_i386 \
    -T /tmp/boot_measure/setup_noassert.ld \
    -o /tmp/boot_measure/setup.elf \
    /tmp/boot_measure/*.o

echo ""
echo "=== RESULTS ==="
echo "Code-only total: $TOTAL bytes"
END_ADDR=$(nm /tmp/boot_measure/setup.elf | awk '/ _end/{print $1}')
END_DEC=$((16#$END_ADDR))
echo "_end = 0x${END_ADDR} = ${END_DEC} bytes"
if [ "$END_DEC" -le 32768 ]; then
    echo "STATUS: PASS — _end <= 0x8000 (32KB limit)"
    echo "Margin: $((32768 - END_DEC)) bytes under limit"
else
    echo "STATUS: FAIL — _end > 0x8000 by $((END_DEC - 32768)) bytes"
fi
echo ""
echo "Section layout:"
size -A /tmp/boot_measure/setup.elf
```

## Common Mistakes

1. **Missing `-include compiler_types.h`** — produces smaller but wrong code.
   If your numbers are significantly lower than expected, this is probably why.

2. **Missing `-D_SETUP`** — cpucheck.c and cpuflags.c fail to compile or
   produce wrong code (different headers included).

3. **Missing `/tmp/boot_compat.h`** — compilation fails with undefined macros
   like `likely`, `IS_ENABLED`, `BUILD_BUG_ON`, `__packed`, `__init`.

4. **Using `--gc-sections`** — artificially reduces _end. Kernel doesn't use it.

5. **Using `awk '/^\.text/'`** — misses `.inittext` sections. Use `awk '/text/'`.

6. **Using Berkeley `size` (no `-A`)** — lumps .rodata into "text" column.
   Always use `size -A` for accurate code-only measurement.

7. **Ad-hoc compilation commands** — always use the flags from this document
   verbatim. Do not write new compilation scripts from memory.

8. **Wrong branch** — the peephole optimizations are on `feat/i686-Os`.
   On `main`, you will get larger code (no IRC allocator, no peephole suite).

9. **Forgetting `make ARCH=x86 headers_install`** — without this, generated
   headers like `autoconf.h` and `zoffset.h` don't exist. Compilation fails
   with missing includes.

10. **Comparing pre-fix numbers to post-fix numbers** — the classify_line
    comment stripping fix (2026-02-10) changed optimization behavior, producing
    correct but larger code. Historical numbers from before this fix are not
    directly comparable to current numbers.
