---
name: ship-release
description: >-
  Ship a new Knots release by verifying release readiness on main, previewing
  pending changesets and commits, running required quality gates, merging or
  validating the Version Packages PR, and confirming the GitHub Release assets
  are published.
---

# /ship-release

Ship a new Knots release.

## Use this skill when

- The user asks to cut, ship, publish, or verify a new Knots release.
- The user wants help with the Changesets-driven version PR and release flow.
- The user needs recovery help when a version bump merged but the GitHub
  release or assets did not publish.

## Steps

1. **Confirm release state on `main`** — Work from a clean `main` branch and
   pull the latest remote state first. Check the current version in
   `Cargo.toml` and `package.json`, then compare it with the latest git tag.
   Preview commits since the last tag with:
   `git log $(git describe --tags --abbrev=0)..HEAD --oneline`

2. **Inspect pending release inputs** — Check whether there are unreleased
   changesets in `.changeset/` and whether a `Version Packages` PR already
   exists or was recently merged. For local version consistency, run:
   `npm run check-cargo-version`

3. **Decide the path** —
   - If user-facing changes are on `main` and no version PR exists yet, let the
     Changesets workflow create or update the `Version Packages` PR.
   - If the `Version Packages` PR exists, review the bump, changelog, and
     version sync before merging it.
   - If the version PR is already merged and `Cargo.toml` is ahead of the
     latest release tag, move to release verification or recovery.

4. **Run required quality gates** — Before merging a version PR or retrying a
   release, run:
   - `make sanity`
   - `npm run check-cargo-version`

   If release tooling changed, also run:
   - `scripts/release/smoke-install.sh`

   Stop on any failure. Do not ship a release with failing sanity checks.

5. **Trigger or watch the release** —
   - Normal path: merge the `Version Packages` PR into `main`. The `Release`
     GitHub Actions workflow should trigger automatically on the push.
   - Recovery path: if the version bump is already on `main` but the release is
     missing, inspect the `Release` workflow and re-run it or trigger it with
     `workflow_dispatch`.

6. **Verify published outputs** — Confirm the GitHub Release exists for
   `v<version>` and that these assets are attached:
   - `knots-v<semver>-darwin-arm64.tar.gz`
   - `knots-v<semver>-linux-x86_64.tar.gz`
   - `knots-v<semver>-linux-aarch64.tar.gz`
   - `knots-v<semver>-checksums.txt`

7. **Report** — Tell the user the released version, whether the release was
   newly published or recovered, link the GitHub Release, and call out any
   follow-up work if verification found a problem.

## Notes

- Knots uses Changesets to open a `Version Packages` PR; the release is not cut
  by manually bumping a `patch`, `minor`, or `major` flag in this repo.
- `npm run version-packages` runs `changeset version` and then syncs
  `Cargo.toml` and `package.json`.
- The release workflow publishes GitHub release notes with `--generate-notes`.
- If the workflow reports that `v<version>` already exists on remote, stop and
  treat it as a tag-collision recovery task instead of retrying blindly.
