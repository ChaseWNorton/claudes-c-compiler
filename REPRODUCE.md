# Reproducing Boot Code Size Measurements

## Overview

CCC compiles the Linux kernel's 16-bit real-mode boot code (21 C files from
`arch/x86/boot/`). The goal is to fit the linked image under the 32KB limit
(`_end ≤ 0x8000`). This document describes exactly how to reproduce the
measurements.

**Current results (2026-02-09):**

| Config | Code-only (21 files) | Linked _end | Status |
|--------|---------------------|-------------|--------|
| CCC -Os -mregparm=3, full peephole suite | **23,551 bytes** | **31,120 (0x7990)** | **UNDER 32KB** |
| CCC -Os -mregparm=3, IRC + clobber only | 24,271 bytes | 35,216 (0x8990) | Over (cliff) |
| CCC -Os (no regparm), IRC + clobber only | 27,037 bytes | 35,216 (0x8990) | Over (cliff) |
| GCC -Os -mregparm=3 (reference) | ~10,500 bytes | 22,976 (0x59C0) | Under |

32KB limit = 32,768 bytes (0x8000).

**_end = 0x7990 = 31,120 bytes — 1,648 bytes of margin under the 32KB limit.**

### Optimization journey

| Stage | Code-only | Linked _end | Delta |
|-------|-----------|-------------|-------|
| Linear scan allocator | 29,633 | 39,312 (0x9990) | baseline |
| + IRC graph coloring | 27,109 | 39,312 (0x9990) | -2,524 |
| + IRC clobber refinement | 27,037 | 35,216 (0x8990) | -72 (crossed 0x7000 cliff, -4,096 _end) |
| + -mregparm=3 | 24,271 | 35,216 (0x8990) | -2,766 |
| + symbol+offset folding | 23,851 | 35,216 (0x8990) | -420 |
| + extend-fold (movzbl+movl) | 23,727 | 35,216 (0x8990) | -124 |
| + andl $0 zero-store | **23,551** | **31,120 (0x7990)** | -176 (crossed 0x6000 cliff, -4,096 _end) |

## Prerequisites

- **x86-64 Linux box** (Azure VM, bare metal, or WSL2) with:
  - Rust toolchain (`rustup`, `cargo`)
  - GNU binutils: `ld`, `as`, `cpp`, `nm`, `size` (the standard `binutils` package)
  - `wget`, `make`, `gcc` (for kernel header generation only)
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

**How to tell if you forgot it:** your code-only total will be ~3,000 bytes
lower than expected (around 21,000 instead of 23,551). The code compiles
fine but is WRONG.

### WARNING about -mregparm=3

**`-mregparm=3` is part of `REALMODE_CFLAGS` in the kernel's `arch/x86/Makefile`.**
It passes the first 3 integer arguments in registers (eax, edx, ecx) instead of
on the stack. Without it, all arguments go on the stack, producing significantly
larger code (+2,766 bytes across 21 files). This flag was missing from earlier
measurements, inflating code-only numbers.

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

### 6. Compare against GCC (optional)

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
File                         Code-only
a20                              545
apm                              335
cmdline                         1388
cpu                              642
cpucheck                        2184
cpuflags                         701
early_serial_console            1297
edd                                0
main                             732
memory                           422
pm                               370
printf                          4757
regs                             105
string                          2633
tty                              406
version                            0
video-bios                       760
video-mode                      1404
video-vesa                       991
video-vga                        801
video                           3078
TOTAL                          23551
```

Linked: **`_end = 0x7990 = 31,120 bytes`** (1,648 bytes under the 32KB limit)

Section layout:
```
.bstext         495       0
.entrytext      104     620
.inittext       298     724
.initdata        30    1022
.text         23489    1052
.text32          34   24541
.pecompat         9   24576    ← at 0x6000 (4KB-aligned)
.rodata        1313   24592
.data           140   26000
.bss           4964   26144
Total         31089
```

### IRC + clobber refinement + regparm=3 only (before peephole improvements)

These were the numbers BEFORE the symbol+offset folding, extend-fold, and
andl $0 optimizations. Recorded for reference:

```
File                         Code-only
a20                              577
apm                              343
cmdline                         1394
cpu                              642
cpucheck                        2257
cpuflags                         709
early_serial_console            1351
edd                                0
main                             795
memory                           453
pm                               412
printf                          4785
regs                             111
string                          2713
tty                              415
version                            0
video-bios                       804
video-mode                      1433
video-vesa                      1029
video-vga                        820
video                           3228
TOTAL                          24271
```

Linked: `_end = 0x8990 = 35,216 bytes` (due to 4KB alignment cliff)

### Without -mregparm=3 (for reference)

Remove `-mregparm=3` from the compilation command.

Expected total: **27,037 bytes** code-only. Same linked _end (cliff-dominated).

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
```

Between cliffs, _end does NOT change — savings are absorbed by alignment
padding. Only when code size crosses a 4KB boundary does _end jump.

The current code (23,551 bytes code-only) produces `.text32` ending at byte
24,575, which is just BELOW the 0x6000 boundary. This places `.pecompat` at
0x6000, yielding `_end = 0x7990`.

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
   If your numbers are ~3,000 bytes lower than expected, this is probably why.

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
