# Reproducing Boot Code Size Measurements

## Overview

CCC compiles the Linux kernel's 16-bit real-mode boot code (21 C files from
`arch/x86/boot/`). The goal is to fit the linked image under the 32KB limit
(`_end ≤ 0x8000`). This document describes exactly how to reproduce the
measurements.

**Current results (2026-02-09):**

| Config | Code-only (21 files) | Linked _end |
|--------|---------------------|-------------|
| CCC -Os, IRC + clobber refinement | 27,037 bytes | 35,216 (0x8990) |
| CCC -Os, IRC baseline | 27,109 bytes | 39,312 (0x9990) |
| CCC -Os, linear scan | 29,633 bytes | 39,312 (0x9990) |
| GCC -Os (reference) | ~10,500 bytes | 22,976 (0x59C0) |

32KB limit = 32,768 bytes (0x8000). Gap: 2,448 bytes.

## Prerequisites

- Azure VM (or any x86-64 Linux box) with:
  - Rust toolchain (`rustup`, `cargo`)
  - GNU binutils (`ld`, `as`, `cpp`, `nm`, `size`)
  - Linux 6.9 kernel source with headers installed
- CCC source code (this repository)

### VM setup used for measurements

```
Host: 20.114.191.87 (Azure, Ubuntu)
CCC:  /home/azureuser/ccc
Kernel: /tmp/linux-6.9
```

### Kernel preparation (one-time)

```bash
cd /tmp
wget https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.9.tar.xz
tar xf linux-6.9.tar.xz
cd linux-6.9
make ARCH=x86 defconfig
make ARCH=x86 headers_install
# Verify these exist:
ls include/generated/autoconf.h
ls arch/x86/boot/zoffset.h
```

### boot_compat.h (one-time)

Create `/tmp/boot_compat.h` with kernel attribute macros that CCC supports
but that are normally provided by kernel build infrastructure:

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
cd /home/azureuser/ccc
cargo build --release
```

The binary is at `target/release/ccc`.

## Compilation Flags

### CRITICAL: ALL of these flags are required

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

This exact mistake was made twice during development (2026-02-08, 2026-02-09),
producing the invalid numbers 26,413 / 34,528 that appeared in earlier notes.

## Step-by-Step Reproduction

### 1. Compile all 21 C files with CCC

```bash
cd $KDIR
rm -rf /tmp/boot_measure && mkdir -p /tmp/boot_measure

FILES="a20 apm cmdline cpu cpucheck cpuflags early_serial_console
       edd main memory pm printf regs string tty version
       video-bios video-mode video-vesa video-vga video"

for src in $FILES; do
    $CCC -m16 -Os -c \
        $IFLAGS $DFLAGS $PREAMBLE \
        arch/x86/boot/${src}.c \
        -o /tmp/boot_measure/${src}.o
done
```

All 21 files must compile without errors.

### 2. Measure code-only per file

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

### 3. Assemble the 4 .S files

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

### 4. Link and measure _end

```bash
# Remove the ASSERT so we can measure even when over 32KB
cp arch/x86/boot/setup.ld /tmp/boot_measure/setup_noassert.ld
sed -i '/ASSERT(_end <= 0x8000/d' /tmp/boot_measure/setup_noassert.ld

ld -m elf_i386 \
    -T /tmp/boot_measure/setup_noassert.ld \
    -o /tmp/boot_measure/setup.elf \
    /tmp/boot_measure/*.o

# Check _end
nm /tmp/boot_measure/setup.elf | grep " _end"

# Full section layout
size -A /tmp/boot_measure/setup.elf
```

**Do NOT use `--gc-sections`.** The kernel boot build does not use it.

### 5. Compare against GCC

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

# Same measurement loop as step 2
```

## Expected Results

### IRC allocator + clobber refinement (default under -Os)

```
File                         Code-only
a20                              766
apm                              429
cmdline                         1415
cpu                              724
cpucheck                        2314
cpuflags                         794
early_serial_console            1744
edd                                0
main                             924
memory                           561
pm                               524
printf                          4895
regs                             118
string                          2859
tty                              567
version                            0
video-bios                       945
video-mode                      1570
video-vesa                      1174
video-vga                       1286
video                           3428
TOTAL                          27037
```

Linked: `_end = 0x8990 = 35,216 bytes`

### IRC allocator without clobber refinement (for reference)

To disable clobber refinement, remove the pass-2 block in
`src/backend/graph_coloring.rs` (the `refine_clobber_points` call in `allocate_irc`).

Expected total: **27,109 bytes** code-only. Linked _end: 39,312 (0x9990).

### Linear scan allocator (for comparison)

To force linear scan instead of IRC, edit `src/backend/stack_layout/regalloc_helpers.rs`
line 65: change `allocate_registers_irc` to `allocate_registers`, rebuild, recompile.

Expected total: **29,633 bytes** code-only. Linked _end: 39,312 (0x9990).

### Alignment cliff note

The `.pecompat` section has 4KB alignment. Currently at `0x7000` (crossed down
from `0x8000` thanks to clobber refinement saving 72 bytes of code). The next
cliff at `0x6000` would require ~4KB more code savings. Between cliffs, _end
does NOT change (savings are absorbed by alignment padding).

Note: only 2,448 bytes over the 32KB limit — but the next _end reduction won't
happen until code savings push past the `0x6000` alignment boundary.

## Common Mistakes

1. **Missing `-include compiler_types.h`** — produces smaller but wrong code.
   If your numbers are ~3,000 bytes lower than expected, this is probably why.

2. **Missing `-D_SETUP`** — cpucheck.c and cpuflags.c fail to compile or
   produce wrong code (different headers included).

3. **Using `--gc-sections`** — artificially reduces _end. Kernel doesn't use it.

4. **Using `awk '/^\.text/'`** — misses `.inittext` sections. Use `awk '/text/'`.

5. **Using Berkeley `size` (no `-A`)** — lumps .rodata into "text" column.
   Always use `size -A` for accurate code-only measurement.

6. **Ad-hoc compilation commands** — always use the flags from this document
   verbatim. Do not write new compilation scripts from memory.
