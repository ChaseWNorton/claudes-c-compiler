Triage the CCC issue backlog — analyze, prioritize, and manage.

## Steps

1. **Fetch current state** — run in parallel:
   ```bash
   gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title --limit 50
   gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title,author,updatedAt --limit 50
   gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title --limit 20
   gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title --limit 20
   ```

2. **Categorize** — group issues by priority and status (available/claimed/stale).

3. **Identify action items**:
   - Unprefixed issues that need a priority assignment
   - Stale claims (draft PRs with no activity in 24+ hours)
   - Duplicate issues
   - Issues missing required information

4. **Check chain health** (if `[CC]` PRs exist):
   ```bash
   # Detect chain
   gh pr list --repo anthropics/claudes-c-compiler --state open \
     --json number,title,headRefName,isDraft --limit 100 \
     | jq '[.[] | select(.title | test("^\\[CC\\]"))]'
   ```
   Report:
   - **Chain integrity**: all `[CC]` PRs present and ordered by PR number
   - **Stale chain tip**: tip PR is draft (no one can build on it) or has no activity in 24+ hours
   - **Broken chain**: a `[CC]` PR was closed/denied mid-chain — downstream PRs need rebase
   - **Orphaned chain PRs**: `[CC]` PRs whose base branch no longer exists
   - **Chain length**: if chain is very long (10+ PRs), recommend the maintainer speed-merge the tip

5. **Display the backlog health report**:
   ```
   BACKLOG HEALTH REPORT
   =====================
   CHAIN:
     Status: healthy | broken | stale tip
     Length: X PRs (#N → #N → ... → #N)
     Tip: PR #N — <title>
   NEEDS TRIAGE: ...
   STALE CLAIMS: ...
   PROGRESS: Open XX | Claimed XX | Available XX | Completed XX
   BY PRIORITY: P0: XX, P1: XX, P2: XX, P3: XX
   RECOMMENDATIONS: ...
   ```

6. **Take action** — for each recommendation, ask the user if they want to proceed (reprioritize, close stale PRs, close duplicates, fix chain issues).
