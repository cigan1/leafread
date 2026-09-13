# Releasing leafread

Maintainer runbook. Everything in the "Automated" section happens in CI; the
rest is manual and requires repository permissions.

## Versioning

leafread follows [SemVer](https://semver.org/). While pre-1.0, minor versions
may include breaking CLI changes; patch versions should not.

## Release checklist

1. **Update the version** in `Cargo.toml`.
2. **Update `CHANGELOG.md`**: move `Unreleased` entries under a new
   `## [x.y.z] - YYYY-MM-DD` heading and update the compare links.
3. **Run the local checks**:
   ```sh
   cargo fmt --all --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --all
   cargo build --release
   ```
4. **Commit** (`Release vX.Y.Z`) and merge to `main` via pull request.
5. **Tag and push**:
   ```sh
   git tag -s vX.Y.Z -m "vX.Y.Z"
   git push origin vX.Y.Z
   ```

## Automated by `release.yml`

On a `v*` tag, CI:

- builds release binaries for Linux x86_64, Linux aarch64, macOS x86_64, and
  macOS aarch64,
- attaches `.tar.gz` archives plus SHA-256 checksums to a GitHub Release,
- publishes the crate to crates.io **if** the `CARGO_REGISTRY_TOKEN` secret is
  configured (and only then).

## Required repository secrets

| Secret | Purpose |
|---|---|
| `CARGO_REGISTRY_TOKEN` | crates.io API token scoped to `leafread` publish |

Do not add deploy tokens to workflows that run on `pull_request`; release
workflows only run on tags and are protected by the `release` environment if
configured.

## Homebrew

The tap lives at `cigan1/homebrew-tap`. After a GitHub Release is published,
update the formula from the template in
[`packaging/homebrew/leafread.rb`](packaging/homebrew/leafread.rb):

1. Copy the file into the tap as `Formula/leafread.rb`.
2. Set `version` and the four `sha256` values from the release's
   `checksums.txt`.
3. Open a PR in the tap and merge after CI passes.

## Repository settings (one-time, launch)

Configure these in GitHub → Settings:

- **Default branch**: `main`.
- **Ruleset / branch protection for `main`**:
  - Require a pull request before merging.
  - Require status checks: `PR validation` jobs and `OSS compliance`.
  - Require branches to be up to date before merging.
  - Do **not** require approving reviews (maintainer-only merge is enough; see
    `MAINTAINERS.md`).
  - Restrict who can merge to maintainers.
- **Merge methods**: squash or rebase; disable merge commits.
- **Actions permissions**: "Allow all actions and reusable workflows" is fine,
  but set default `GITHUB_TOKEN` permissions to read-only.
- **Private vulnerability reporting**: enabled (SECURITY.md links to it).
- **Discussions**: enabled (SUPPORT.md points there).
- **Secrets**: add `CARGO_REGISTRY_TOKEN` when you are ready to publish.
- **Teams/collaborators**: ensure the maintainer list in `MAINTAINERS.md`
  matches repository access.

## Post-release

- Verify installation paths: `cargo install leafread`, the release tarball, and
  (after the tap PR) `brew install cigan1/tap/leafread`.
- Check the crates.io page renders the README.
- Announce in Discussions with the changelog summary.

## Resuming a crates.io publish

If the GitHub Release was created before `CARGO_REGISTRY_TOKEN` existed, the
`publish-crate` job skips publishing. To publish that same version afterwards:

```sh
# 1. Create an API token at https://crates.io/settings/tokens with the
#    `publish-update` scope for the `leafread` crate, then:
gh secret set CARGO_REGISTRY_TOKEN --repo cigan1/leafread

# 2. Re-run only the publish job of the release workflow:
gh run rerun <run-id> --repo cigan1/leafread --job <publish-crate-job-id>
```

Alternatively publish from a local checkout of the tag:

```sh
git checkout vX.Y.Z
cargo publish --locked    # after `cargo login`
```
