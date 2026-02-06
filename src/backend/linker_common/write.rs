//! ELF64 binary emission helpers.
//!
//! Common functions for alignment/padding. Used by x86, RISC-V, and ARM linkers.
//! Section/program header writing is handled by `Shdr64`/`Phdr64` structs in `elf::io`.

/// Align `val` up to the next multiple of `align` (power-of-two alignment).
pub fn align_up_64(val: u64, align: u64) -> u64 {
    if align <= 1 { val } else { (val + align - 1) & !(align - 1) }
}

/// Extend buffer with zero bytes to reach `target` length.
pub fn pad_to(buf: &mut Vec<u8>, target: usize) {
    if buf.len() < target { buf.resize(target, 0); }
}
