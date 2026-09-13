# Maintainers

| Name | GitHub | Email | Role |
|---|---|---|---|
| cigan1 | [@cigan1](https://github.com/cigan1) | cigan1@gmail.com | Maintainer (BDFL) |

## What maintainers do

- Triage issues and label them (`bug`, `enhancement`, `documentation`,
  `good first issue`).
- Review and merge pull requests.
- Cut releases (see [RELEASING.md](RELEASING.md)).
- Handle security reports per [SECURITY.md](SECURITY.md).
- Keep the repository settings and CI healthy (branch protection, required
  checks, secrets).

## Merge policy

- All changes land via pull request, including maintainer changes.
- The default branch is protected: pull requests require CI (validation +
  compliance) to pass, and only maintainers may merge.
- Once required checks pass, a maintainer merges without waiting for a second
  review; external contributions are reviewed by a maintainer first.
- Direct pushes to `main` are reserved for emergencies (e.g. a compromised
  dependency), documented in the release notes.

## Becoming a maintainer

Sustained, high-quality contributions and helpful issue triage are the path to
maintainership. A maintainer adds new maintainers by pull request updating this
file, and mirrors the change in repository permissions.
