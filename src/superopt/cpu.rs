//! CPU state model for the superoptimizer.
//!
//! Simulates a subset of the i686 CPU: 8 general-purpose registers,
//! 5 arithmetic flags, and a 256-byte stack memory window centered on EBP.

use super::ir::REG_EBP;

/// Stack window size: 4096 bytes, indexed as ebp-2048..ebp+2047.
/// Must be large enough for CCC's stack frames. CCC normalizes ESP-relative
/// offsets as EBP-relative, and ESP offsets in real boot code reach ~1060.
const STACK_SIZE: usize = 4096;
/// Stack base offset: index 0 in the stack array corresponds to ebp - STACK_BASE.
const STACK_BASE: i32 = 2048;

/// Simulated i686 CPU state.
#[derive(Clone)]
pub struct CpuState {
    /// General-purpose registers: eax(0), ecx(1), edx(2), ebx(3),
    /// esp(4), ebp(5), esi(6), edi(7).
    pub regs: [u32; 8],
    /// Carry flag.
    pub cf: bool,
    /// Zero flag.
    pub zf: bool,
    /// Sign flag.
    pub sf: bool,
    /// Overflow flag.
    pub of: bool,
    /// Parity flag (even parity of low byte).
    pub pf: bool,
    /// Stack memory window.
    pub stack: [u8; STACK_SIZE],
    /// Whether the state is poisoned (undefined behavior occurred).
    pub poisoned: bool,
}

impl CpuState {
    /// Create a new state with the given register values.
    /// EBP is set to a fixed base address so ebp-relative offsets work.
    /// Stack is initialized to zero.
    pub fn new(regs: [u32; 8]) -> Self {
        CpuState {
            regs,
            cf: false,
            zf: false,
            sf: false,
            of: false,
            pf: false,
            stack: [0u8; STACK_SIZE],
            poisoned: false,
        }
    }

    /// Create a random state for fuzzing.
    pub fn random(rng: &mut Rng) -> Self {
        let mut regs = [0u32; 8];
        for r in &mut regs {
            *r = rng.next_u32();
        }
        // Fix EBP to a known value so stack offsets are consistent.
        regs[REG_EBP as usize] = 0x0000_1000;
        // Fix ESP to something reasonable.
        regs[4] = 0x0000_0F00;

        let mut stack = [0u8; STACK_SIZE];
        for byte in &mut stack {
            *byte = rng.next_u32() as u8;
        }

        CpuState {
            regs,
            cf: rng.next_u32() & 1 != 0,
            zf: rng.next_u32() & 1 != 0,
            sf: rng.next_u32() & 1 != 0,
            of: rng.next_u32() & 1 != 0,
            pf: rng.next_u32() & 1 != 0,
            stack,
            poisoned: false,
        }
    }

    /// Read a 32-bit value from the stack at an ebp-relative offset.
    pub fn read_stack32(&self, offset: i32) -> Option<u32> {
        let raw = offset + STACK_BASE;
        if raw < 0 { return None; }
        let idx = raw as usize;
        if idx + 3 >= STACK_SIZE {
            return None;
        }
        Some(u32::from_le_bytes([
            self.stack[idx],
            self.stack[idx + 1],
            self.stack[idx + 2],
            self.stack[idx + 3],
        ]))
    }

    /// Write a 32-bit value to the stack at an ebp-relative offset.
    pub fn write_stack32(&mut self, offset: i32, val: u32) -> Option<()> {
        let raw = offset + STACK_BASE;
        if raw < 0 { return None; }
        let idx = raw as usize;
        if idx + 3 >= STACK_SIZE {
            return None;
        }
        let bytes = val.to_le_bytes();
        self.stack[idx] = bytes[0];
        self.stack[idx + 1] = bytes[1];
        self.stack[idx + 2] = bytes[2];
        self.stack[idx + 3] = bytes[3];
        Some(())
    }

    /// Read an 8-bit value from the stack at an ebp-relative offset.
    pub fn read_stack8(&self, offset: i32) -> Option<u8> {
        let raw = offset + STACK_BASE;
        if raw < 0 { return None; }
        let idx = raw as usize;
        if idx >= STACK_SIZE {
            return None;
        }
        Some(self.stack[idx])
    }

    /// Write an 8-bit value to the stack at an ebp-relative offset.
    pub fn write_stack8(&mut self, offset: i32, val: u8) -> Option<()> {
        let raw = offset + STACK_BASE;
        if raw < 0 { return None; }
        let idx = raw as usize;
        if idx >= STACK_SIZE {
            return None;
        }
        self.stack[idx] = val;
        Some(())
    }

    /// Compare two states for equivalence according to an output mask.
    pub fn equivalent(&self, other: &CpuState, mask: &OutputMask) -> bool {
        // Poisoned states match anything (both UB = acceptable).
        if self.poisoned && other.poisoned {
            return true;
        }
        if self.poisoned != other.poisoned {
            return false;
        }

        // Check registers.
        for i in 0..8 {
            if mask.regs[i] && self.regs[i] != other.regs[i] {
                return false;
            }
        }

        // Check flags.
        if mask.flags {
            if self.cf != other.cf || self.zf != other.zf || self.sf != other.sf
                || self.of != other.of || self.pf != other.pf
            {
                return false;
            }
        }

        // Check stack slots.
        for &off in &mask.stack_offsets {
            if self.read_stack32(off) != other.read_stack32(off) {
                return false;
            }
        }

        true
    }
}

/// Which parts of the CPU state must match for two sequences to be equivalent.
#[derive(Debug, Clone)]
pub struct OutputMask {
    /// Which registers are live outputs (must match).
    pub regs: [bool; 8],
    /// Whether flags are live outputs.
    pub flags: bool,
    /// Stack offsets (ebp-relative) that are live outputs.
    pub stack_offsets: Vec<i32>,
}

impl OutputMask {
    /// Mask where all registers and flags must match (most conservative).
    pub fn all() -> Self {
        OutputMask {
            regs: [true; 8],
            flags: true,
            stack_offsets: Vec::new(),
        }
    }

    /// Mask for specific registers and flags.
    pub fn regs_and_flags(regs: &[u8], flags: bool) -> Self {
        let mut mask = OutputMask {
            regs: [false; 8],
            flags,
            stack_offsets: Vec::new(),
        };
        for &r in regs {
            mask.regs[r as usize] = true;
        }
        mask
    }
}

/// Minimal xorshift64 PRNG (zero external dependencies).
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Ensure non-zero state.
        Rng { state: if seed == 0 { 1 } else { seed } }
    }

    pub fn next_u32(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_read_write() {
        let mut cpu = CpuState::new([0; 8]);
        cpu.write_stack32(-8, 0xDEADBEEF).unwrap();
        assert_eq!(cpu.read_stack32(-8), Some(0xDEADBEEF));
    }

    #[test]
    fn test_stack_boundary() {
        let cpu = CpuState::new([0; 8]);
        // Should succeed within the 4096-byte window (ebp-2048..ebp+2047).
        assert!(cpu.read_stack32(-2000).is_some());
        assert!(cpu.read_stack32(1500).is_some());
        // Should fail outside the window.
        assert!(cpu.read_stack32(-2049).is_none());
        assert!(cpu.read_stack32(2045).is_none());
    }

    #[test]
    fn test_equivalence_matching_mask() {
        let mut a = CpuState::new([1, 2, 3, 4, 5, 6, 7, 8]);
        let mut b = CpuState::new([1, 99, 3, 4, 5, 6, 7, 8]);
        // Only check eax — should match despite ecx differing.
        let mask = OutputMask::regs_and_flags(&[0], false);
        assert!(a.equivalent(&b, &mask));
        // Check ecx too — should fail.
        let mask2 = OutputMask::regs_and_flags(&[0, 1], false);
        assert!(!a.equivalent(&b, &mask2));
    }

    #[test]
    fn test_rng_produces_varied_values() {
        let mut rng = Rng::new(12345);
        let a = rng.next_u32();
        let b = rng.next_u32();
        let c = rng.next_u32();
        assert_ne!(a, b);
        assert_ne!(b, c);
    }
}
