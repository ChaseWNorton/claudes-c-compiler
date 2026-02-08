//! Instruction IR for the superoptimizer.
//!
//! Compact representation of i686 instructions used for enumeration,
//! emulation, and equivalence checking. Not raw text — structured types
//! that can be efficiently compared, hashed, and serialized.

use std::fmt;

// Register indices matching the i686 convention.
pub const REG_EAX: u8 = 0;
pub const REG_ECX: u8 = 1;
pub const REG_EDX: u8 = 2;
pub const REG_EBX: u8 = 3;
pub const REG_ESP: u8 = 4;
pub const REG_EBP: u8 = 5;
pub const REG_ESI: u8 = 6;
pub const REG_EDI: u8 = 7;

pub const REG_NAMES: [&str; 8] = ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"];

/// Instruction mnemonics supported by the superoptimizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mnemonic {
    Movl,
    Addl,
    Subl,
    Xorl,
    Andl,
    Orl,
    Cmpl,
    Testl,
    Negl,
    Notl,
    Incl,
    Decl,
    Shll,
    Shrl,
    Sarl,
    Leal,
    Imull,
    Movzbl,
    Movsbl,
    Cltd,
    Pushl,
    Popl,
    Xchgl,
}

impl Mnemonic {
    /// AT&T syntax mnemonic string.
    pub fn as_str(self) -> &'static str {
        match self {
            Mnemonic::Movl => "movl",
            Mnemonic::Addl => "addl",
            Mnemonic::Subl => "subl",
            Mnemonic::Xorl => "xorl",
            Mnemonic::Andl => "andl",
            Mnemonic::Orl => "orl",
            Mnemonic::Cmpl => "cmpl",
            Mnemonic::Testl => "testl",
            Mnemonic::Negl => "negl",
            Mnemonic::Notl => "notl",
            Mnemonic::Incl => "incl",
            Mnemonic::Decl => "decl",
            Mnemonic::Shll => "shll",
            Mnemonic::Shrl => "shrl",
            Mnemonic::Sarl => "sarl",
            Mnemonic::Leal => "leal",
            Mnemonic::Imull => "imull",
            Mnemonic::Movzbl => "movzbl",
            Mnemonic::Movsbl => "movsbl",
            Mnemonic::Cltd => "cltd",
            Mnemonic::Pushl => "pushl",
            Mnemonic::Popl => "popl",
            Mnemonic::Xchgl => "xchgl",
        }
    }

    /// All mnemonics useful for replacement enumeration (excludes push/pop/cltd
    /// which are rarely useful as replacements in 2-instruction windows).
    pub fn enumerable() -> &'static [Mnemonic] {
        &[
            Mnemonic::Movl,
            Mnemonic::Addl,
            Mnemonic::Subl,
            Mnemonic::Xorl,
            Mnemonic::Andl,
            Mnemonic::Orl,
            Mnemonic::Negl,
            Mnemonic::Notl,
            Mnemonic::Incl,
            Mnemonic::Decl,
            Mnemonic::Shll,
            Mnemonic::Shrl,
            Mnemonic::Sarl,
            Mnemonic::Leal,
            Mnemonic::Imull,
            Mnemonic::Testl,
            Mnemonic::Cmpl,
        ]
    }

    /// Number of operands this mnemonic takes.
    pub fn num_operands(self) -> u8 {
        match self {
            Mnemonic::Negl | Mnemonic::Notl | Mnemonic::Incl | Mnemonic::Decl
            | Mnemonic::Pushl | Mnemonic::Popl => 1,
            Mnemonic::Cltd => 0,
            _ => 2,
        }
    }

    /// Whether the mnemonic is commutative (operand order doesn't matter for result).
    pub fn is_commutative(self) -> bool {
        matches!(self, Mnemonic::Addl | Mnemonic::Xorl | Mnemonic::Andl
            | Mnemonic::Orl | Mnemonic::Testl | Mnemonic::Imull)
    }
}

/// Operand for the superoptimizer instruction IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SOperand {
    /// General-purpose register (0=eax .. 7=edi).
    Reg(u8),
    /// Immediate constant.
    Imm(i32),
    /// Memory at ebp-relative offset: `offset(%ebp)`.
    MemEbp(i32),
    /// LEA-style memory operand: `offset(%base, %index, scale)`.
    /// Encoded as (base_reg, index_reg, scale, displacement).
    LeaMem(u8, u8, u8, i32),
    /// No operand (unused slot).
    None,
}

impl SOperand {
    pub fn is_reg(self) -> bool {
        matches!(self, SOperand::Reg(_))
    }

    pub fn is_mem(self) -> bool {
        matches!(self, SOperand::MemEbp(_) | SOperand::LeaMem(..))
    }

    pub fn reg_id(self) -> Option<u8> {
        match self {
            SOperand::Reg(r) => Some(r),
            _ => None,
        }
    }
}

/// A single instruction in the superoptimizer IR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SInstr {
    pub mnemonic: Mnemonic,
    pub operands: [SOperand; 3],
    pub num_operands: u8,
}

impl SInstr {
    pub fn new0(mnemonic: Mnemonic) -> Self {
        SInstr {
            mnemonic,
            operands: [SOperand::None, SOperand::None, SOperand::None],
            num_operands: 0,
        }
    }

    pub fn new1(mnemonic: Mnemonic, op0: SOperand) -> Self {
        SInstr {
            mnemonic,
            operands: [op0, SOperand::None, SOperand::None],
            num_operands: 1,
        }
    }

    pub fn new2(mnemonic: Mnemonic, op0: SOperand, op1: SOperand) -> Self {
        SInstr {
            mnemonic,
            operands: [op0, op1, SOperand::None],
            num_operands: 2,
        }
    }

    pub fn new3(mnemonic: Mnemonic, op0: SOperand, op1: SOperand, op2: SOperand) -> Self {
        SInstr {
            mnemonic,
            operands: [op0, op1, op2],
            num_operands: 3,
        }
    }

    /// Render as AT&T syntax string (e.g., "    addl %ecx, %eax").
    pub fn to_att(&self) -> String {
        let mut s = format!("    {}", self.mnemonic.as_str());
        for i in 0..self.num_operands as usize {
            if i > 0 { s.push_str(", "); } else { s.push(' '); }
            match self.operands[i] {
                SOperand::Reg(r) => {
                    s.push('%');
                    s.push_str(REG_NAMES[r as usize]);
                }
                SOperand::Imm(v) => {
                    s.push('$');
                    s.push_str(&v.to_string());
                }
                SOperand::MemEbp(off) => {
                    s.push_str(&off.to_string());
                    s.push_str("(%ebp)");
                }
                SOperand::LeaMem(base, index, scale, disp) => {
                    if disp != 0 { s.push_str(&disp.to_string()); }
                    s.push('(');
                    s.push('%');
                    s.push_str(REG_NAMES[base as usize]);
                    s.push_str(", %");
                    s.push_str(REG_NAMES[index as usize]);
                    s.push_str(", ");
                    s.push_str(&scale.to_string());
                    s.push(')');
                }
                SOperand::None => {}
            }
        }
        s
    }
}

impl fmt::Display for SInstr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_att())
    }
}

/// A sequence of instructions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Program {
    pub instrs: Vec<SInstr>,
}

impl Program {
    pub fn new(instrs: Vec<SInstr>) -> Self {
        Program { instrs }
    }

    pub fn len(&self) -> usize {
        self.instrs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instrs.is_empty()
    }

    /// Render the full program as AT&T assembly lines.
    pub fn to_att(&self) -> String {
        self.instrs.iter().map(|i| i.to_att()).collect::<Vec<_>>().join("\n")
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_att())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_att_rendering_reg_reg() {
        let instr = SInstr::new2(Mnemonic::Addl, SOperand::Reg(REG_ECX), SOperand::Reg(REG_EAX));
        assert_eq!(instr.to_att(), "    addl %ecx, %eax");
    }

    #[test]
    fn test_att_rendering_imm_reg() {
        let instr = SInstr::new2(Mnemonic::Movl, SOperand::Imm(42), SOperand::Reg(REG_EAX));
        assert_eq!(instr.to_att(), "    movl $42, %eax");
    }

    #[test]
    fn test_att_rendering_mem_reg() {
        let instr = SInstr::new2(Mnemonic::Movl, SOperand::MemEbp(-8), SOperand::Reg(REG_EAX));
        assert_eq!(instr.to_att(), "    movl -8(%ebp), %eax");
    }

    #[test]
    fn test_att_rendering_unary() {
        let instr = SInstr::new1(Mnemonic::Incl, SOperand::Reg(REG_EAX));
        assert_eq!(instr.to_att(), "    incl %eax");
    }

    #[test]
    fn test_att_rendering_zero_operands() {
        let instr = SInstr::new0(Mnemonic::Cltd);
        assert_eq!(instr.to_att(), "    cltd");
    }

    #[test]
    fn test_program_display() {
        let p = Program::new(vec![
            SInstr::new2(Mnemonic::Xorl, SOperand::Reg(REG_EAX), SOperand::Reg(REG_EAX)),
            SInstr::new2(Mnemonic::Addl, SOperand::Imm(1), SOperand::Reg(REG_EAX)),
        ]);
        assert_eq!(p.to_att(), "    xorl %eax, %eax\n    addl $1, %eax");
    }
}
