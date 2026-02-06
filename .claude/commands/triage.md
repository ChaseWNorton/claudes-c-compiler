Triage the CCC issue backlog — analyze, prioritize, and manage.

## Steps

1. **Fetch current state** — run in parallel:
   ```bash
   gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title,body --limit 50
   gh pr list --repo anthropics/claudes-c-compiler --state open --json number,title,body,author,updatedAt --limit 50
   gh issue list --repo anthropics/claudes-c-compiler --state closed --json number,title --limit 20
   gh pr list --repo anthropics/claudes-c-compiler --state merged --json number,title --limit 20
   ```

2. **Categorize** — group issues by priority and status (available/claimed/stale).

3. **Identify action items**:
   - Unprefixed issues that need a priority assignment
   - Stale claims (draft PRs with no activity in 24+ hours)
   - Duplicate issues
   - Issues missing required information

4. **Display the backlog health report**:
   ```
   BACKLOG HEALTH REPORT
   =====================
   NEEDS TRIAGE: ...
   STALE CLAIMS: ...
   PROGRESS: Open XX | Claimed XX | Available XX | Completed XX
   BY PRIORITY: P0: XX, P1: XX, P2: XX, P3: XX
   RECOMMENDATIONS: ...
   ```

5. **Take action** — for each recommendation, ask the user if they want to proceed (reprioritize, close stale PRs, close duplicates).
