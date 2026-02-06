# Release — Cut Releases from Merged Work

Package merged work into a release with a changelog, git tag, and GitHub release.

## When to Use

Use this skill when:
- A milestone is complete (all issues closed, all PRs merged)
- A significant batch of fixes has been merged
- You want to create a snapshot of the compiler's current state

## Workflow

### 1. Determine release scope

```bash
# Find the last release tag (if any)
gh release list --repo anthropics/claudes-c-compiler --limit 5

# If no releases exist, all merged PRs are in scope
# If releases exist, find PRs merged since the last tag
git log <LAST_TAG>..HEAD --oneline
```

### 2. Gather merged PRs

```bash
# All merged PRs (or since last release date)
gh pr list --repo anthropics/claudes-c-compiler --state merged \
  --json number,title,body,mergedAt --limit 50
```

### 3. Categorize changes

Group merged PRs by type (parse from PR title and body):

| Category | PR title pattern | Changelog section |
|----------|-----------------|-------------------|
| Bug fixes | `Fix #N:` | Bug Fixes |
| Diagnostics | mentions error/warning | Diagnostics |
| Infrastructure | CI, tooling, commands | Infrastructure |
| Testing | test, coverage | Testing |
| Documentation | doc, comment, README | Documentation |
| Performance | optimize, perf | Performance |

### 4. Generate changelog

Format:

```markdown
# Release v<VERSION> — <Title>

<1-2 sentence summary of what this release achieves>

## Bug Fixes
- Fix #20: Detect duplicate case labels in switch (#PR)
- Fix #21: Detect duplicate default labels (#PR)

## Diagnostics
- Fix #22: Diagnose void function returning a value (#PR)
- Fix #23: Error on case/default outside switch (#PR)

## Infrastructure
- Add CI workflow for build and test (#PR)
- Add community contribution skills and commands (#PR)

## Testing
- Add sema test infrastructure with helpers (#PR)

## Statistics
- X PRs merged
- Y issues closed
- Z new tests added
```

### 5. Determine version number

Follow semantic versioning relative to the project's maturity:

- **Pre-1.0** (current): use `0.X.0` where X increments for each milestone completed
- **Post-1.0** (future): use semver properly (major.minor.patch)

If no version scheme exists yet, start with `v0.1.0`.

### 6. Create the release

```bash
# Create and push the tag
git tag -a v<VERSION> -m "Release v<VERSION>: <title>"
git push origin v<VERSION>

# Create GitHub release
gh release create v<VERSION> \
  --repo anthropics/claudes-c-compiler \
  --title "v<VERSION>: <Title>" \
  --notes "$(cat <<'EOF'
<changelog content from step 4>
EOF
)"
```

### 7. Close completed milestones (maintainer only)

Check each open `[MILESTONE]` issue: find all sub-issues (bodies containing
`Part of [MILESTONE]` referencing it). If all sub-issues are closed, the milestone
is complete — close it:

```bash
gh issue close <MILESTONE_NUMBER> --repo anthropics/claudes-c-compiler \
  --comment "Completed in release v<VERSION>. All sub-issues resolved."
```

### 8. Report

```
RELEASE v<VERSION>: <Title>
===========================

Tag: v<VERSION>
URL: <github release URL>

Changes:
  Bug fixes: X
  Diagnostics: Y
  Infrastructure: Z
  Testing: W

Milestones completed: M<N>

Next: Run /roadmap to plan the next cycle.
```

## Release Cadence

Suggested release triggers:
- **Milestone complete** — when all issues in a `[MILESTONE]` are closed
- **Batch threshold** — when 10+ PRs have been merged without a release
- **Time-based** — weekly/monthly if there's been activity

## Pre-Release Checklist

Before cutting a release:

- [ ] `cargo build --release` passes on latest main
- [ ] `cargo test --lib` passes with zero failures
- [ ] No open draft PRs with `Fixes #N` for issues that should be in this release
- [ ] Changelog accurately reflects all merged changes
- [ ] Version number follows the versioning scheme
