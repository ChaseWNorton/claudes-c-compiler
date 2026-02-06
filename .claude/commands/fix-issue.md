Work on fixing GitHub issue #$ARGUMENTS from the anthropics/claudes-c-compiler repository.

## Workflow

1. **Fetch the issue details:**
   ```
   gh issue view $ARGUMENTS --repo anthropics/claudes-c-compiler
   ```

2. **Read the issue body** to understand the problem, reproduction steps, suggested approach, and files to modify.

3. **Create a feature branch:**
   ```
   git switch -c fix/issue-$ARGUMENTS
   ```

4. **Read the files** mentioned in the issue to understand the existing code.

5. **Implement the fix** following the suggested approach in the issue. Follow the patterns described in CLAUDE.md.

6. **Write tests** as described in the issue. Use the existing test helpers:
   - `sema_errors("C code")` / `sema_warnings("C code")` for frontend diagnostic tests
   - `compile_to_ir("C code")` for IR-level tests
   - Add tests in `#[cfg(test)] mod tests` at the bottom of the modified file

7. **Verify:**
   ```
   cargo build --release && cargo test --lib
   ```

8. **Commit** with a descriptive message:
   ```
   git commit -m "Fix #$ARGUMENTS: <short description>"
   ```

9. **Push and create a PR:**
   ```
   git push -u origin fix/issue-$ARGUMENTS
   gh pr create --repo anthropics/claudes-c-compiler --title "Fix #$ARGUMENTS: <title>" --body "..."
   ```

## PR requirements

- Title references the issue number
- Body has: Summary, Changes, and Test plan sections
- Body ends with: `Fixes #$ARGUMENTS`
- All existing tests pass + new tests for the fix
- Clean build with no new warnings
