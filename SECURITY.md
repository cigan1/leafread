# Security Policy

## Supported versions

leafread is pre-1.0. Security fixes are applied to the latest release on the
`main` branch; older releases may not receive patches.

| Version | Supported |
|---|---|
| latest 0.x | ✅ |
| older releases | ❌ |

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Report privately to **cigan1@gmail.com** or via
[GitHub private vulnerability reporting](https://github.com/cigan1/leafread/security/advisories/new).

Include, when possible:

- affected version (`leafread --version`) and platform,
- a description of the issue and its impact,
- a minimal reproduction (a Markdown file and command are ideal),
- any suggested fix.

You can expect an acknowledgement within 5 business days. Once a fix is
available we will coordinate disclosure and credit you in the release notes
unless you prefer otherwise.

## Scope

Security-relevant surfaces in this project include:

- parsing of untrusted Markdown (memory safety, pathological inputs),
- file access from relative links and images,
- terminal escape sequence injection from document content,
- the `open` integration for links (external command invocation).

Out of scope: rendering quality issues, missing Markdown features, and problems
that require an already-compromised machine.

## Supply chain

- PR checks run with read-only permissions and never receive release secrets.
- Releases are built from tagged commits in CI and published to GitHub
  Releases; crates.io publishing uses a scoped token stored as an encrypted
  repository secret.
- Dependency licenses and advisories are checked in CI with `cargo-deny`
  (see [`deny.toml`](deny.toml)).
