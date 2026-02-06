Review the open issues on this repository and help me pick one to work on.

First, fetch the open issues:
```
gh issue list --repo anthropics/claudes-c-compiler --state open --json number,title,body --limit 30
```

Present the issues grouped by priority (look for [P0], [P1], [P2], [P3] prefixes in titles):
- **P0 (Critical)** — correctness bugs that any C compiler should catch
- **P1 (High)** — important bugs and infrastructure gaps
- **P2 (Medium)** — correctness and feature gaps
- **P3 (Low)** — testing and nice-to-have improvements

For each issue show:
- Issue number and title
- A one-line summary of effort required (small/medium/large)

Then ask which issue I'd like to work on.
