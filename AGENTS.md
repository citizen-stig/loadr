# Branching policy

- NEVER base implementation, fix, performance, or refactoring work on `risotto` or `origin/risotto`.
- `risotto` is integration-only.
- Before starting work, identify the owning feature branch from Git history and make the change there.
- Test and review work on its feature branch before merging it into `risotto`.
- If a defect was introduced by an integration conflict, repair or reconstruct the owning feature branch first; do not create a risotto-based cleanup branch.
