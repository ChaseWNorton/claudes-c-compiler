# Review Fix — PR Review Skill

Review an incoming pull request against its linked issue's acceptance criteria.

## When to Use

Use this skill when:
- A PR has been marked ready for review
- You want to validate that a fix actually addresses the issue
- You want to check code quality before merging

## Workflow

### 1. Fetch PR and linked issue

```bash
# Get PR details
gh pr view <PR_NUMBER> --repo anthropics/claudes-c-compiler --json title,body,files

# Extract the linked issue number from "Fixes #N" in the PR body
# Then fetch the issue
gh issue view <ISSUE_NUMBER> --repo anthropics/claudes-c-compiler
```

### 2. Read the diff

```bash
gh pr diff <PR_NUMBER> --repo anthropics/claudes-c-compiler
```

### 3. Review against checklist

For each item, mark PASS or FAIL:

**Correctness:**
- [ ] The fix addresses the problem described in the issue
- [ ] The reproduction case from the issue would be handled correctly
- [ ] Edge cases mentioned in the issue are covered
- [ ] No regressions introduced (existing behavior preserved)

**Code quality:**
- [ ] Changes are in the files suggested by the issue (or have good reason not to be)
- [ ] Follows existing code patterns (look at surrounding code)
- [ ] No unnecessary changes outside the scope of the fix
- [ ] Error messages match GCC/Clang format where applicable

**Tests:**
- [ ] New tests are present and cover the fix
- [ ] Tests are in `#[cfg(test)] mod tests` at the bottom of the modified file
- [ ] Tests use the appropriate helpers (`sema_error_count`, `compile_to_ir`, etc.)
- [ ] Test cases match what the issue specified

**PR hygiene:**
- [ ] PR title follows format: `Fix #N: <short description>`
- [ ] PR body has Summary, Changes, and Test plan sections
- [ ] PR body ends with `Fixes #N`
- [ ] Single commit (or clean commit history)

### 4. Verify locally (optional but recommended)

```bash
# Fetch the PR branch
gh pr checkout <PR_NUMBER>

# Build and test
cargo build --release && cargo test --lib
```

### 5. Submit review

**If all checks pass:**
```bash
gh pr review <PR_NUMBER> --repo anthropics/claudes-c-compiler --approve \
  --body "LGTM. All acceptance criteria met."
```

**If issues found:**
```bash
gh pr review <PR_NUMBER> --repo anthropics/claudes-c-compiler --request-changes \
  --body "$(cat <<'EOF'
## Review

### Issues found:
- <specific issue 1>
- <specific issue 2>

### Suggested fixes:
- <what to change>

Please address and push updates.
EOF
)"
```

## Review Priorities

Focus on what matters most, in order:

1. **Does it fix the bug?** — The #1 question. If the fix doesn't work, nothing else matters.
2. **Does it break anything?** — Check for regressions in the diff.
3. **Are tests present?** — A fix without tests will regress.
4. **Is the code clean?** — Minor style issues can be fixed later; correctness can't.

## Reference Files

- **[CHECKLIST.md](CHECKLIST.md)** — Detailed review checklist by issue category
