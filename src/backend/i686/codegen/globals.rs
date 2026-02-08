//! I686Codegen: global address operations (global, label, TLS).

use crate::ir::reexports::Value;
use crate::emit;
use super::emit::{I686Codegen, phys_reg_name, phys_reg_to_cache_idx};

impl I686Codegen {
    pub(super) fn emit_global_addr_impl(&mut self, dest: &Value, name: &str) {
        let (target, direct) = match self.dest_reg(dest) {
            Some(p) => (phys_reg_name(p), true),
            None => ("eax", false),
        };
        if self.state.pic_mode {
            if self.state.needs_got(name) {
                emit!(self.state, "    movl {}@GOT(%ebx), %{}", name, target);
            } else {
                emit!(self.state, "    leal {}@GOTOFF(%ebx), %{}", name, target);
            }
        } else {
            emit!(self.state, "    movl ${}, %{}", name, target);
        }
        if direct {
            let dest_phys = self.dest_reg(dest).unwrap();
            if let Some(idx) = phys_reg_to_cache_idx(dest_phys) {
                self.state.reg_cache.set_reg(idx, dest.0, false);
            }
        } else {
            self.state.reg_cache.invalidate_acc();
            self.store_eax_to(dest);
        }
    }

    pub(super) fn emit_label_addr_impl(&mut self, dest: &Value, label: &str) {
        let (target, direct) = match self.dest_reg(dest) {
            Some(p) => (phys_reg_name(p), true),
            None => ("eax", false),
        };
        if self.state.pic_mode {
            emit!(self.state, "    leal {}@GOTOFF(%ebx), %{}", label, target);
        } else {
            emit!(self.state, "    movl ${}, %{}", label, target);
        }
        if direct {
            let dest_phys = self.dest_reg(dest).unwrap();
            if let Some(idx) = phys_reg_to_cache_idx(dest_phys) {
                self.state.reg_cache.set_reg(idx, dest.0, false);
            }
        } else {
            self.state.reg_cache.invalidate_acc();
            self.store_eax_to(dest);
        }
    }

    pub(super) fn emit_tls_global_addr_impl(&mut self, dest: &Value, name: &str) {
        let (target, direct) = match self.dest_reg(dest) {
            Some(p) => (phys_reg_name(p), true),
            None => ("eax", false),
        };
        if self.state.pic_mode {
            emit!(self.state, "    movl {}@GOTNTPOFF(%ebx), %{}", name, target);
            emit!(self.state, "    addl %gs:0, %{}", target);
        } else {
            emit!(self.state, "    movl %gs:0, %{}", target);
            emit!(self.state, "    addl ${}@NTPOFF, %{}", name, target);
        }
        if direct {
            let dest_phys = self.dest_reg(dest).unwrap();
            if let Some(idx) = phys_reg_to_cache_idx(dest_phys) {
                self.state.reg_cache.set_reg(idx, dest.0, false);
            }
        } else {
            self.state.reg_cache.invalidate_acc();
            self.store_eax_to(dest);
        }
    }
}
