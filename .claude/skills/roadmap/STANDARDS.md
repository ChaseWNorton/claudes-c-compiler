# C11 Standards Coverage

Reference for roadmap planning: what CCC implements vs what it should implement.

## Key C11 Sections

### 6.2 — Concepts
| Section | Topic | CCC Status |
|---------|-------|-----------|
| 6.2.1 | Scopes of identifiers | Implemented |
| 6.2.2 | Linkages of identifiers | Partial — conflicting linkage detected |
| 6.2.5 | Types | Implemented (basic, pointer, array, struct, union, enum, function) |
| 6.2.7 | Compatible type | Partial — not fully checked in sema |

### 6.3 — Conversions
| Section | Topic | CCC Status |
|---------|-------|-----------|
| 6.3.1.1 | Boolean, characters, integers | Implemented |
| 6.3.2.2 | void | Partial — void return not fully diagnosed |
| 6.3.2.3 | Pointers | Partial — implicit conversions warned |

### 6.5 — Expressions
| Section | Topic | CCC Status |
|---------|-------|-----------|
| 6.5.2 | Postfix operators | Implemented |
| 6.5.3 | Unary operators | Implemented |
| 6.5.4 | Cast operators | Implemented |
| 6.5.16 | Assignment operators | Partial — const assignment detected |

### 6.7 — Declarations
| Section | Topic | CCC Status |
|---------|-------|-----------|
| 6.7.1 | Storage-class specifiers | Implemented — conflicting specifiers detected |
| 6.7.2 | Type specifiers | Implemented |
| 6.7.3 | Type qualifiers | Partial — const checked, volatile/restrict parsed |
| 6.7.4 | Function specifiers | Implemented (inline) |
| 6.7.6 | Declarators | Implemented |
| 6.7.8 | Initialization | Implemented |
| 6.7.9 | Designated initializers | Implemented |

### 6.8 — Statements
| Section | Topic | CCC Status |
|---------|-------|-----------|
| 6.8.1 | Labeled statements | Partial — duplicate case/default not detected |
| 6.8.4 | Selection statements | Implemented |
| 6.8.4.2 | switch | Partial — case outside switch not diagnosed |
| 6.8.5 | Iteration statements | Implemented |
| 6.8.6 | Jump statements | Partial — return type not fully checked |

### 6.9 — External definitions
| Section | Topic | CCC Status |
|---------|-------|-----------|
| 6.9.1 | Function definitions | Implemented |
| 6.9.2 | External object definitions | Partial — tentative definitions not merged |

### 6.10 — Preprocessing
| Section | Topic | CCC Status |
|---------|-------|-----------|
| 6.10.1 | Conditional inclusion | Implemented |
| 6.10.2 | Source file inclusion | Implemented |
| 6.10.3 | Macro replacement | Implemented |
| 6.10.8 | Predefined macro names | Implemented |

## GCC Warning Parity

Key `-W` flags and CCC's support:

| Flag | GCC behavior | CCC Status |
|------|-------------|-----------|
| `-Wall` | Enable common warnings | Partial set implemented |
| `-Wextra` | Extra warnings | Not implemented |
| `-Werror` | Warnings as errors | Implemented |
| `-Wreturn-type` | Missing return | Implemented (warn only) |
| `-Wunused-variable` | Unused variables | Not implemented |
| `-Wshadow` | Variable shadowing | Not implemented |
| `-Wpointer-arith` | sizeof on function types | Implemented |
| `-Wimplicit-function-declaration` | Implicit function decl | Implemented |
| `-Wshift-count-overflow` | Shift too large | Implemented |
| `-Wdiv-by-zero` | Division by zero | Implemented |
| `-Woverflow` | Integer overflow | Implemented |

## Areas for Roadmap Milestones

Based on the gaps above, natural milestones would be:
1. **Statement-level diagnostics** — duplicate case/default, case outside switch, return type checking
2. **Declaration-level diagnostics** — tentative definitions, type compatibility, void return
3. **Warning expansion** — -Wunused-variable, -Wshadow, -Wextra coverage
4. **Type system completeness** — full type compatibility checking, qualifier tracking
5. **Test coverage** — every module should have unit tests
