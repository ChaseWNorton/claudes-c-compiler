Analyze the CCC compiler and create a strategic roadmap with milestones.

## Instructions

1. **Assess current state**:
   - Read CLAUDE.md and README.md for project context
   - Fetch open/closed issues and merged PRs to understand what's been done
   - Check for existing `[MILESTONE]` issues

2. **Identify strategic gaps**:
   - Compare CCC's diagnostics against GCC `-Wall` coverage
   - Check test coverage across modules
   - Identify reliability gaps (CLI, error handling)
   - Review C11 standard compliance

3. **Create milestones** as GitHub issues with `[MILESTONE]` prefix:
   - Each milestone has: Goal, Scope, Issues checklist, Success criteria
   - Sequence milestones by priority and dependencies
   - Use `M1`, `M2`, `M3` numbering

4. **Report** the roadmap: milestones in order, with descriptions and estimated scope.

5. **Recommend** running `/decompose` on the first milestone to create actionable issues.

See the roadmap skill for the full workflow and reference files (STANDARDS.md, BENCHMARKS.md).
