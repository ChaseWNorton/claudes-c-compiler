//! X86Codegen: cast operations.

use crate::ir::reexports::{IrConst, Operand, Value};
use crate::common::types::IrType;
use crate::backend::generation::is_i128_type;
use super::emit::X86Codegen;

impl X86Codegen {
    pub(super) fn emit_cast_instrs_impl(&mut self, from_ty: IrType, to_ty: IrType) {
        self.emit_cast_instrs_x86(from_ty, to_ty);
    }

    pub(super) fn emit_cast_impl(&mut self, dest: &Value, src: &Operand, from_ty: IrType, to_ty: IrType) {
        // Intercept i128/u128 -> F128: call __floattixf/__floatuntixf, result in st(0).
        if to_ty == IrType::F128 && is_i128_type(from_ty) {
            if let Some(dest_slot) = self.state.get_slot(dest.0) {
                self.operand_to_rax_rdx(src);
                self.state.emit("    movq %rax, %rdi");
                self.state.emit("    movq %rdx, %rsi");
                let func = if from_ty.is_signed() { "__floattixf" } else { "__floatuntixf" };
                self.state.emit_fmt(format_args!("    call {}@PLT", func));
                self.state.reg_cache.invalidate_all();
                // Result in st(0) — store full 80-bit, keep f64 copy in rax
                self.state.out.emit_instr_rbp("    fstpt", dest_slot.0);
                self.state.out.emit_instr_rbp("    fldt", dest_slot.0);
                self.state.emit("    subq $8, %rsp");
                self.state.emit("    fstpl (%rsp)");
                self.state.emit("    popq %rax");
                self.state.reg_cache.set_acc(dest.0, false);
                self.state.f128_direct_slots.insert(dest.0);
                return;
            }
        }

        // Intercept F128 -> i128/u128: load x87, push as stack arg, call __fixxfti/__fixunsxfti.
        if from_ty == IrType::F128 && is_i128_type(to_ty) {
            self.emit_f128_load_to_x87(src);
            // SysV ABI: long double args passed in memory (on stack)
            self.state.emit("    subq $16, %rsp");
            self.state.emit("    fstpt (%rsp)");
            let func = if to_ty.is_signed() { "__fixxfti" } else { "__fixunsxfti" };
            self.state.emit_fmt(format_args!("    call {}@PLT", func));
            self.state.emit("    addq $16, %rsp");
            self.state.reg_cache.invalidate_all();
            // Result in rax:rdx
            self.store_rax_rdx_to(dest);
            return;
        }

        // Intercept casts TO F128: produce full 80-bit x87 value in dest slot.
        if to_ty == IrType::F128 && from_ty != IrType::F128 && !is_i128_type(from_ty) {
            if let Some(dest_slot) = self.state.get_slot(dest.0) {
                if from_ty == IrType::F64 {
                    self.operand_to_rax(src);
                    self.state.emit("    subq $8, %rsp");
                    self.state.emit("    movq %rax, (%rsp)");
                    self.state.emit("    fldl (%rsp)");
                    self.state.emit("    addq $8, %rsp");
                } else if from_ty == IrType::F32 {
                    self.operand_to_rax(src);
                    self.state.emit("    subq $4, %rsp");
                    self.state.emit("    movl %eax, (%rsp)");
                    self.state.emit("    flds (%rsp)");
                    self.state.emit("    addq $4, %rsp");
                } else if from_ty.is_signed() || (!from_ty.is_float() && !from_ty.is_unsigned()) {
                    self.operand_to_rax(src);
                    if from_ty.size() < 8 {
                        self.emit_cast_instrs_x86(from_ty, IrType::I64);
                    }
                    self.state.emit("    subq $8, %rsp");
                    self.state.emit("    movq %rax, (%rsp)");
                    self.state.emit("    fildq (%rsp)");
                    self.state.emit("    addq $8, %rsp");
                } else {
                    self.operand_to_rax(src);
                    if from_ty.size() < 8 {
                        self.emit_cast_instrs_x86(from_ty, IrType::I64);
                    }
                    let big_label = self.state.fresh_label("u2f128_big");
                    let done_label = self.state.fresh_label("u2f128_done");
                    self.state.emit("    testq %rax, %rax");
                    self.state.out.emit_jcc_label("    js", &big_label);
                    self.state.emit("    subq $8, %rsp");
                    self.state.emit("    movq %rax, (%rsp)");
                    self.state.emit("    fildq (%rsp)");
                    self.state.emit("    addq $8, %rsp");
                    self.state.out.emit_jmp_label(&done_label);
                    self.state.out.emit_named_label(&big_label);
                    self.state.emit("    subq $8, %rsp");
                    self.state.emit("    movq %rax, (%rsp)");
                    self.state.emit("    fildq (%rsp)");
                    self.state.emit("    addq $8, %rsp");
                    self.state.emit("    subq $16, %rsp");
                    self.state.out.emit_instr_imm_reg("    movabsq", -9223372036854775808i64, "rax");
                    self.state.emit("    movq %rax, (%rsp)");
                    self.state.out.emit_instr_imm_reg("    movq", 0x403Fi64, "rax");
                    self.state.emit("    movq %rax, 8(%rsp)");
                    self.state.emit("    fldt (%rsp)");
                    self.state.emit("    addq $16, %rsp");
                    self.state.emit("    faddp %st, %st(1)");
                    self.state.out.emit_named_label(&done_label);
                }
                self.state.out.emit_instr_rbp("    fstpt", dest_slot.0);
                self.state.out.emit_instr_rbp("    fldt", dest_slot.0);
                self.state.emit("    subq $8, %rsp");
                self.state.emit("    fstpl (%rsp)");
                self.state.emit("    popq %rax");
                self.state.reg_cache.set_acc(dest.0, false);
                self.state.f128_direct_slots.insert(dest.0);
                return;
            }
        }

        // Intercept F128 -> F64/F32 casts
        if from_ty == IrType::F128 && (to_ty == IrType::F64 || to_ty == IrType::F32) {
            self.emit_f128_load_to_x87(src);
            if to_ty == IrType::F64 {
                self.state.emit("    subq $8, %rsp");
                self.state.emit("    fstpl (%rsp)");
                self.state.emit("    movq (%rsp), %rax");
                self.state.emit("    addq $8, %rsp");
            } else {
                self.state.emit("    subq $4, %rsp");
                self.state.emit("    fstps (%rsp)");
                self.state.emit("    movl (%rsp), %eax");
                self.state.emit("    addq $4, %rsp");
            }
            self.state.reg_cache.invalidate_acc();
            self.store_rax_to(dest);
            return;
        }

        // Intercept F128 -> integer casts when we know the source's memory location
        if from_ty == IrType::F128 && !to_ty.is_float() && !is_i128_type(to_ty) {
            if let Operand::Value(v) = src {
                if self.state.f128_direct_slots.contains(&v.0) {
                    if let Some(slot) = self.state.get_slot(v.0) {
                        let addr = crate::backend::state::SlotAddr::Direct(slot);
                        self.emit_f128_to_int_from_memory(&addr, to_ty);
                        self.store_rax_to(dest);
                        return;
                    }
                }
                if let Some((ptr_id, _offset, _is_indirect)) = self.state.get_f128_source(v.0) {
                    if let Some(addr) = self.state.resolve_slot_addr(ptr_id) {
                        self.emit_f128_to_int_from_memory(&addr, to_ty);
                        self.store_rax_to(dest);
                        return;
                    }
                }
            }
            if let Operand::Const(IrConst::LongDouble(_, f128_bytes)) = src {
                let x87 = crate::common::long_double::f128_bytes_to_x87_bytes(f128_bytes);
                self.state.emit("    subq $16, %rsp");
                let lo = u64::from_le_bytes(x87[0..8].try_into().unwrap());
                let hi = u16::from_le_bytes(x87[8..10].try_into().unwrap());
                self.state.out.emit_instr_imm_reg("    movabsq", lo as i64, "rax");
                self.state.emit("    movq %rax, (%rsp)");
                self.state.out.emit_instr_imm_reg("    movq", hi as i64, "rax");
                self.state.emit("    movq %rax, 8(%rsp)");
                self.state.emit("    fldt (%rsp)");
                self.state.emit("    addq $16, %rsp");
                self.emit_f128_st0_to_int(to_ty);
                self.store_rax_to(dest);
                return;
            }
        }
        // Fall through to default implementation for all other cases
        crate::backend::traits::emit_cast_default(self, dest, src, from_ty, to_ty);
    }
}

#[cfg(test)]
mod tests {
    use crate::common::error::DiagnosticEngine;
    use crate::common::source::SourceManager;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;
    use crate::frontend::preprocessor::Preprocessor;
    use crate::frontend::sema::SemanticAnalyzer;
    use crate::ir::lowering::Lowerer;
    use crate::ir::mem2reg::promote_allocas;
    use crate::backend::Target;
    use crate::backend::CodegenOptions;

    /// Compile C source all the way through to x86-64 assembly.
    fn compile_to_x86_asm(source: &str) -> String {
        let mut pp = Preprocessor::new();
        let preprocessed = pp.preprocess(source);

        let mut sm = SourceManager::new();
        let fid = sm.add_file("test.c".into(), preprocessed);
        sm.build_line_map();
        let mut lexer = Lexer::new(sm.get_content(fid), fid);
        let tokens = lexer.tokenize();

        let mut diagnostics = DiagnosticEngine::new();
        diagnostics.set_source_manager(sm);
        let mut parser = Parser::new(tokens);
        parser.set_diagnostics(diagnostics);
        let ast = parser.parse();
        assert_eq!(parser.error_count, 0, "parse errors");

        let diagnostics = parser.take_diagnostics();
        let mut sema = SemanticAnalyzer::new();
        sema.set_diagnostics(diagnostics);
        sema.analyze(&ast).expect("sema errors");
        let sema_result = sema.into_result();

        let diagnostics = DiagnosticEngine::new();
        let lowerer = Lowerer::with_type_context(
            Target::X86_64,
            sema_result.type_context,
            sema_result.functions,
            sema_result.expr_types,
            sema_result.const_values,
            diagnostics,
            false,
        );
        let (mut module, _diag) = lowerer.lower(&ast);
        promote_allocas(&mut module);

        let opts = CodegenOptions {
            pic: false,
            function_return_thunk: false,
            indirect_branch_thunk: false,
            patchable_function_entry: None,
            cf_protection_branch: false,
            no_sse: false,
            general_regs_only: false,
            code_model_kernel: false,
            no_jump_tables: false,
            no_relax: false,
            debug_info: false,
            function_sections: false,
            data_sections: false,
            code16gcc: false,
            regparm: 0,
            omit_frame_pointer: false,
            emit_cfi: false,
            optimize_size: false,
        };
        Target::X86_64.generate_assembly_with_opts_and_debug(&module, &opts, None)
    }

    #[test]
    fn i128_to_long_double_no_panic() {
        // Issue #75: i128 → F128 conversion panicked in i128_ops.rs
        let asm = compile_to_x86_asm(
            "__int128 g; long double f(void) { return g; }"
        );
        assert!(asm.contains("__floattixf"), "should call __floattixf for signed i128→F128");
    }

    #[test]
    fn u128_to_long_double_no_panic() {
        let asm = compile_to_x86_asm(
            "unsigned __int128 g; long double f(void) { return g; }"
        );
        assert!(asm.contains("__floatuntixf"), "should call __floatuntixf for unsigned i128→F128");
    }

    #[test]
    fn long_double_to_i128_no_panic() {
        // Issue #77: F128 → i128 conversion panicked in i128_ops.rs
        let asm = compile_to_x86_asm(
            "long double g; __int128 f(void) { return g; }"
        );
        assert!(asm.contains("__fixxfti"), "should call __fixxfti for F128→signed i128");
    }

    #[test]
    fn long_double_to_u128_no_panic() {
        let asm = compile_to_x86_asm(
            "long double g; unsigned __int128 f(void) { return g; }"
        );
        assert!(asm.contains("__fixunsxfti"), "should call __fixunsxfti for F128→unsigned i128");
    }
}
