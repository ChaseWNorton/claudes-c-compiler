# Audit Areas

Detailed guide for auditing each area of the CCC codebase.

## Semantic Analysis (`src/frontend/sema/analysis.rs`)

**What to look for:**
- Missing diagnostics that GCC `-Wall -Wextra` would catch
- Type compatibility checks that are skipped
- Statement-level validation gaps (switch, return, goto)
- Declaration-level validation gaps (conflicting types, redefinitions)

**Methodology:**
1. List every `analyze_*` method
2. For each, check what C11 constraints it should enforce
3. Write C programs that violate each constraint
4. Test with CCC and GCC — note differences

**Known gaps** (from previous analysis):
- Duplicate case/default labels (#20, #21)
- Void function return (#22)
- case outside switch (#23)
- Return type mismatch (#24)

## Lexer (`src/frontend/lexer/scan.rs`)

**What to look for:**
- Unicode escape validation (known gap: #30)
- Character literal validation
- String literal edge cases
- Numeric literal overflow
- Trigraph/digraph handling

**Methodology:**
1. Read each `lex_*` method
2. Check for TODO comments (there are several)
3. Test edge cases: `'\x00'`, `'\777'`, `L'\U0010FFFF'`

## CLI (`src/driver/cli.rs`)

**What to look for:**
- Flags that consume next arg but don't check if it exists
- Numeric parsing with `unwrap_or(0)` instead of error reporting
- Response file (`@file`) error handling
- Flag combinations that should conflict but don't

**Methodology:**
1. Read the `parse_args()` match statement
2. For each flag that takes an argument, verify error handling
3. Test: `ccc -MF` (no argument), `ccc -mregparm=abc` (invalid number)

## Backend Codegen (`src/backend/*/codegen.rs`)

**What to look for:**
- Incorrect instruction selection for specific types
- Calling convention violations (wrong registers, wrong stack layout)
- Missing support for specific operand combinations
- Incorrect handling of struct return values

**Methodology:**
1. Focus on one backend at a time
2. Read the codegen for a specific area (e.g., function calls, struct handling)
3. Write C programs that exercise the specific path
4. Compile with `-S` and inspect the assembly
5. Compare with GCC output

## Type System (`src/common/types.rs`)

**What to look for:**
- Type compatibility checking gaps
- Qualifier propagation issues (const, volatile, restrict)
- Enum underlying type calculation
- Struct/union layout edge cases
- Function type compatibility

**Methodology:**
1. Read the type comparison/equality functions
2. Test type compatibility: `int*` vs `const int*`, `int[5]` vs `int[]`, etc.
3. Check struct layout calculations against GCC's `-fdump-record-layouts`

## Optimization Passes (`src/passes/`)

**What to look for:**
- Incorrect transformations (changes program semantics)
- Missed optimization opportunities
- Infinite loops in pass iteration
- Interactions between passes

**Methodology:**
1. Read each pass's transformation logic
2. Construct IR that exercises edge cases
3. Run the pass and verify the output is semantically correct
4. Test with `CCC_DISABLE_PASSES=<pass>` to isolate issues

## IR Lowering (`src/ir/lowering/`)

**What to look for:**
- Missing C constructs (everything should lower to IR)
- Incorrect lowering of complex expressions
- Switch statement lowering edge cases
- Struct/union member access lowering

**Methodology:**
1. Write C programs with complex constructs
2. Use `compile_to_ir()` test helper to inspect the output
3. Verify the IR correctly represents the C semantics
