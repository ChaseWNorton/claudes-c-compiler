Break milestone #$ARGUMENTS into concrete, actionable GitHub issues.

## Instructions

1. **Read the milestone issue**:
   ```bash
   gh issue view $ARGUMENTS --repo anthropics/claudes-c-compiler
   ```

2. **Analyze the relevant codebase** based on the milestone's scope.

3. **Break into individual issues** — each one should be:
   - One PR, one fix, one concept
   - Self-contained (no dependencies on other issues in the milestone)
   - Testable with clear acceptance criteria

4. **Check for duplicates** before filing:
   ```bash
   gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50
   ```

5. **File each issue** with the standard template (Problem, Expected behavior, Reproduction, Suggested approach, Files to modify, Tests, Acceptance criteria). End each issue body with: `Part of [MILESTONE] M<N> (#$ARGUMENTS)`

6. **Update the milestone issue** with a checklist of the new issues.

7. **Report** what was filed and suggest running `/fix-next` to start working.

See the decompose skill for the full workflow.
