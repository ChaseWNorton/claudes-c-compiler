//! Binary read/write helpers for little-endian ELF fields, plus section header,
//! program header, symbol table entry, and relocation entry writers.

// ── Binary read helpers (little-endian) ──────────────────────────────────────

/// Read a little-endian u16 from `data` at `offset`.
#[inline]
pub fn read_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

/// Read a little-endian u32 from `data` at `offset`.
#[inline]
pub fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
    ])
}

/// Read a little-endian u64 from `data` at `offset`.
#[inline]
pub fn read_u64(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
        data[offset + 4], data[offset + 5], data[offset + 6], data[offset + 7],
    ])
}

/// Read a little-endian i32 from `data` at `offset`.
#[inline]
pub fn read_i32(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
    ])
}

/// Read a little-endian i64 from `data` at `offset`.
#[inline]
pub fn read_i64(data: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes([
        data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
        data[offset + 4], data[offset + 5], data[offset + 6], data[offset + 7],
    ])
}

/// Read a null-terminated string from a byte slice starting at `offset`.
pub fn read_cstr(data: &[u8], offset: usize) -> String {
    if offset >= data.len() {
        return String::new();
    }
    let end = data[offset..].iter().position(|&b| b == 0).unwrap_or(data.len() - offset);
    String::from_utf8_lossy(&data[offset..offset + end]).into_owned()
}

// ── Binary write helpers (little-endian, in-place) ───────────────────────────

/// Write a little-endian u16 into `buf` at `offset`. No-op if out of bounds.
#[inline]
pub fn w16(buf: &mut [u8], off: usize, val: u16) {
    if off + 2 <= buf.len() {
        buf[off..off + 2].copy_from_slice(&val.to_le_bytes());
    }
}

/// Write a little-endian u32 into `buf` at `offset`. No-op if out of bounds.
#[inline]
pub fn w32(buf: &mut [u8], off: usize, val: u32) {
    if off + 4 <= buf.len() {
        buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
    }
}

/// Write a little-endian u64 into `buf` at `offset`. No-op if out of bounds.
#[inline]
pub fn w64(buf: &mut [u8], off: usize, val: u64) {
    if off + 8 <= buf.len() {
        buf[off..off + 8].copy_from_slice(&val.to_le_bytes());
    }
}

/// Copy `data` into `buf` starting at `off`. No-op if out of bounds.
#[inline]
pub fn write_bytes(buf: &mut [u8], off: usize, data: &[u8]) {
    let end = off + data.len();
    if end <= buf.len() {
        buf[off..end].copy_from_slice(data);
    }
}

// ── Section header writing ───────────────────────────────────────────────────

/// ELF64 section header fields.
pub struct Shdr64 {
    pub sh_name: u32, pub sh_type: u32, pub sh_flags: u64,
    pub sh_addr: u64, pub sh_offset: u64, pub sh_size: u64,
    pub sh_link: u32, pub sh_info: u32, pub sh_addralign: u64, pub sh_entsize: u64,
}

impl Shdr64 {
    pub fn append_to(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.sh_name.to_le_bytes());
        buf.extend_from_slice(&self.sh_type.to_le_bytes());
        buf.extend_from_slice(&self.sh_flags.to_le_bytes());
        buf.extend_from_slice(&self.sh_addr.to_le_bytes());
        buf.extend_from_slice(&self.sh_offset.to_le_bytes());
        buf.extend_from_slice(&self.sh_size.to_le_bytes());
        buf.extend_from_slice(&self.sh_link.to_le_bytes());
        buf.extend_from_slice(&self.sh_info.to_le_bytes());
        buf.extend_from_slice(&self.sh_addralign.to_le_bytes());
        buf.extend_from_slice(&self.sh_entsize.to_le_bytes());
    }
}

/// ELF32 section header fields.
pub struct Shdr32 {
    pub sh_name: u32, pub sh_type: u32, pub sh_flags: u32,
    pub sh_addr: u32, pub sh_offset: u32, pub sh_size: u32,
    pub sh_link: u32, pub sh_info: u32, pub sh_addralign: u32, pub sh_entsize: u32,
}

impl Shdr32 {
    pub fn append_to(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.sh_name.to_le_bytes());
        buf.extend_from_slice(&self.sh_type.to_le_bytes());
        buf.extend_from_slice(&self.sh_flags.to_le_bytes());
        buf.extend_from_slice(&self.sh_addr.to_le_bytes());
        buf.extend_from_slice(&self.sh_offset.to_le_bytes());
        buf.extend_from_slice(&self.sh_size.to_le_bytes());
        buf.extend_from_slice(&self.sh_link.to_le_bytes());
        buf.extend_from_slice(&self.sh_info.to_le_bytes());
        buf.extend_from_slice(&self.sh_addralign.to_le_bytes());
        buf.extend_from_slice(&self.sh_entsize.to_le_bytes());
    }
}

/// ELF64 program header fields.
pub struct Phdr64 {
    pub p_type: u32, pub p_flags: u32, pub p_offset: u64,
    pub p_vaddr: u64, pub p_paddr: u64, pub p_filesz: u64, pub p_memsz: u64, pub p_align: u64,
}

impl Phdr64 {
    pub fn write_at(&self, buf: &mut [u8], off: usize) {
        w32(buf, off, self.p_type);
        w32(buf, off + 4, self.p_flags);
        w64(buf, off + 8, self.p_offset);
        w64(buf, off + 16, self.p_vaddr);
        w64(buf, off + 24, self.p_paddr);
        w64(buf, off + 32, self.p_filesz);
        w64(buf, off + 40, self.p_memsz);
        w64(buf, off + 48, self.p_align);
    }

    pub fn append_to(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.p_type.to_le_bytes());
        buf.extend_from_slice(&self.p_flags.to_le_bytes());
        buf.extend_from_slice(&self.p_offset.to_le_bytes());
        buf.extend_from_slice(&self.p_vaddr.to_le_bytes());
        buf.extend_from_slice(&self.p_paddr.to_le_bytes());
        buf.extend_from_slice(&self.p_filesz.to_le_bytes());
        buf.extend_from_slice(&self.p_memsz.to_le_bytes());
        buf.extend_from_slice(&self.p_align.to_le_bytes());
    }
}

/// Write an ELF64 symbol table entry to `buf`.
pub fn write_sym64(
    buf: &mut Vec<u8>,
    st_name: u32, st_info: u8, st_other: u8, st_shndx: u16,
    st_value: u64, st_size: u64,
) {
    buf.extend_from_slice(&st_name.to_le_bytes());
    buf.push(st_info);
    buf.push(st_other);
    buf.extend_from_slice(&st_shndx.to_le_bytes());
    buf.extend_from_slice(&st_value.to_le_bytes());
    buf.extend_from_slice(&st_size.to_le_bytes());
}

/// Write an ELF32 symbol table entry to `buf`.
pub fn write_sym32(
    buf: &mut Vec<u8>,
    st_name: u32, st_value: u32, st_size: u32,
    st_info: u8, st_other: u8, st_shndx: u16,
) {
    buf.extend_from_slice(&st_name.to_le_bytes());
    buf.extend_from_slice(&st_value.to_le_bytes());
    buf.extend_from_slice(&st_size.to_le_bytes());
    buf.push(st_info);
    buf.push(st_other);
    buf.extend_from_slice(&st_shndx.to_le_bytes());
}

/// Write an ELF64 RELA relocation entry to `buf`.
pub fn write_rela64(buf: &mut Vec<u8>, r_offset: u64, r_sym: u32, r_type: u32, r_addend: i64) {
    buf.extend_from_slice(&r_offset.to_le_bytes());
    let r_info: u64 = ((r_sym as u64) << 32) | (r_type as u64);
    buf.extend_from_slice(&r_info.to_le_bytes());
    buf.extend_from_slice(&r_addend.to_le_bytes());
}

/// Write an ELF32 REL relocation entry to `buf`.
pub fn write_rel32(buf: &mut Vec<u8>, r_offset: u32, r_sym: u32, r_type: u8) {
    buf.extend_from_slice(&r_offset.to_le_bytes());
    let r_info: u32 = (r_sym << 8) | (r_type as u32);
    buf.extend_from_slice(&r_info.to_le_bytes());
}

/// Write an ELF32 RELA relocation entry to `buf`.
/// Used by architectures that require RELA even in 32-bit mode (e.g., RISC-V).
pub fn write_rela32(buf: &mut Vec<u8>, r_offset: u32, r_sym: u32, r_type: u8, r_addend: i32) {
    buf.extend_from_slice(&r_offset.to_le_bytes());
    let r_info: u32 = (r_sym << 8) | (r_type as u32);
    buf.extend_from_slice(&r_info.to_le_bytes());
    buf.extend_from_slice(&r_addend.to_le_bytes());
}
