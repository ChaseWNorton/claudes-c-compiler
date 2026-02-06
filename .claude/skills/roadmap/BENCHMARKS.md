# Benchmarks and Real-World Build Results

Reference for roadmap planning: what CCC can and can't build, and why.

## Projects That Build and Pass Tests

Per README, these compile and pass their test suites:
- PostgreSQL (237 regression tests)
- SQLite
- QuickJS
- zlib
- Lua
- libsodium
- libpng
- jq
- libjpeg-turbo
- mbedTLS
- libuv
- Redis
- libffi
- musl
- TCC
- DOOM

Additional projects that build:
- FFmpeg (7331 FATE checkasm tests on x86-64 and AArch64)
- GNU coreutils
- Busybox
- CPython
- QEMU
- LuaJIT

## Known Limitations (from README)

- **Optimization levels**: All -O levels run the same pipeline (no tiered optimization)
- **Long double**: x86 80-bit works via x87; ARM/RISC-V use soft-float
- **Complex numbers**: `_Complex` has edge-case failures
- **GNU extensions**: Partial `__attribute__` support
- **Atomics**: `_Atomic` parsed but qualifier not tracked through type system
- **NEON**: Partially implemented (core 128-bit operations)

## Build Failure Categories

When CCC fails to build a project, it's typically due to:

1. **Missing GNU extensions** — `__attribute__((cleanup))`, `__typeof__`, statement expressions
2. **Missing C11 features** — `_Generic`, `_Static_assert` edge cases
3. **Assembly integration** — inline asm with complex constraints
4. **Platform-specific code** — code that assumes GCC-specific behavior
5. **Preprocessor edge cases** — complex macro expansion, variadic macros

## Quality Metrics to Track

For roadmap milestones, these metrics indicate progress:

| Metric | How to measure | Current baseline |
|--------|---------------|-----------------|
| Unit tests | `cargo test --lib` count | ~602 tests |
| Diagnostic coverage | Count of GCC -Wall warnings we match | Unknown — audit needed |
| Build success | Projects that compile | 150+ (per README) |
| Test pass rate | Projects whose test suites pass | ~20 (per README) |
| C11 section coverage | Sections fully implemented | Partial — see STANDARDS.md |

## Using This for Roadmap Planning

When creating milestones, reference this data:
- "M1 will close the diagnostic gap for -Wall warnings" (quantifiable)
- "M2 will fix build failures in projects that fail due to type checking" (measurable)
- "M3 will add test coverage to modules with zero tests" (concrete)

Avoid vague milestones like "improve the compiler" — tie each one to a measurable outcome.
