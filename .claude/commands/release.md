Cut a release from merged work on the CCC compiler.

## Instructions

1. **Determine scope** — find all PRs merged since the last release (or all merged PRs if no releases exist):
   ```bash
   gh release list --repo anthropics/claudes-c-compiler --limit 5
   gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title,mergedAt --limit 50
   ```

2. **Categorize changes** — group PRs into: Bug Fixes, Diagnostics, Infrastructure, Testing, Documentation.

3. **Generate changelog** with statistics (PRs merged, issues closed, tests added).

4. **Verify quality**:
   ```bash
   cargo build --release && cargo test --lib
   ```

5. **Create the release** — tag + GitHub release with changelog.

6. **Close completed milestones** if this release finishes any.

7. **Report** the release URL and recommend running `/roadmap` for the next cycle.

See the release skill for the full workflow and version numbering scheme.
