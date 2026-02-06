Deep audit of a CCC module — find every gap and file issues.

## Instructions

If `$ARGUMENTS` is provided, audit that specific file or module path.
Otherwise, pick the highest-value audit target.

1. **Read every file** in the target module, line by line.

2. **Look for**:
   - `unwrap()` / `unwrap_or()` on user input (should report errors)
   - `// TODO` and `// FIXME` comments (known gaps)
   - Missing diagnostics (compare against GCC behavior)
   - Missing error handling (silent failures)
   - Missing test coverage (no `#[cfg(test)]` module, or sparse tests)
   - Incorrect logic (compare against C11 standard)

3. **Write reproduction cases** — minimal C code that demonstrates each issue.

4. **Check for duplicates** before filing.

5. **File issues** for every gap found, using the standard template with priority prefixes.

6. **Report** the full audit results: files read, issues found, issues filed.

See the audit skill for the full methodology and AUDIT_AREAS.md for per-module guidance.
