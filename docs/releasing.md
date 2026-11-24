# Releasing flare

A push of a tag `v<version>` runs the Release workflow,
[.github/workflows/release.yml](../.github/workflows/release.yml). The
workflow builds `flare` for macOS on Apple silicon and for Linux on arm64 and
x86_64, and attaches `flare-<version>-<target>.tar.gz` for each target and a
`SHA256SUMS` file to the tag's GitHub release. The crumbkit Homebrew tap
reads the latest release of each public crumbkit repository every hour, and
writes `Formula/flare.rb` from these files.

## Installing flare

A colleague installs flare with Homebrew on macOS or Linux:

```sh
brew install crumbkit/tap/flare
flare --version
```

`brew upgrade flare` installs a later release. The tap writes the new
formula up to an hour after the release appears. Running the tap's Update
formulas workflow by hand writes it at once.

The Linux binaries are built on Ubuntu 24.04 and need glibc 2.39 or later.
There is no build for Intel Macs or Windows. On Linux, `flare login` refuses
with `flare::keychain::unavailable` unless `FLARE_KEYCHAIN` names a program
that stores the token, because Linux has no macOS Keychain.

## Cutting a release

Only the owner cuts a release.

1. Land a pull request that sets the workspace version in Cargo.toml,
   updates Cargo.lock with `cargo update --workspace` so that it records the
   new version, and moves CHANGELOG.md's Unreleased entries under that
   version. Every build runs with `--locked`, so a pull request that changes
   Cargo.toml without Cargo.lock fails CI. The version has no pre-release
   part such as `-rc.1`, because the `version` job refuses one.
2. Run the full audit that rule `/rules/hangar/full-audit-before-release`
   requires. Every high or medium finding is fixed or recorded as an
   accepted risk before the tag.
3. Tag the merge commit on main:

   ```sh
   board call github.tag --name v<version> --target <merge commit SHA> \
     --message 'flare <version>'
   ```

4. Watch the Release run in the repository's Actions tab. The release
   appears when every job passes.

## When a release run fails

A re-run of a job builds the same tagged commit with the release.yml that the
tag holds. A re-run therefore fixes only a failure that a second attempt can
pass, and a fix on main reaches a release only through a new tag. Each
failure below takes one of two recoveries.

- **Re-run the failed jobs** from the run's page in the Actions tab.
- **Retag**: delete the tag, merge the fix to main, and tag the new merge
  commit with `board call github.tag` as in step 3. `board call github.tag`
  never deletes or moves a tag, so delete it with git:

  ```sh
  git push origin :refs/tags/v<version>
  ```

Delete and retag a version only while it has no published release. Once a
release for the version is published, the tap may already have written its
formula and colleagues may have installed it, so land a pull request that
raises the version and tag that version.

| Failure | No release exists? | Recovery |
| --- | --- | --- |
| A job was cancelled because no runner picked it up, or a download or the GitHub API failed for a moment | Yes | Re-run the failed jobs |
| The `version` job refuses the tag: it differs from Cargo.toml, the version has a `-`, or the tagged commit is not on main | Yes | Retag |
| The `advisories` job finds an advisory. Update the dependency, or record an ignore with a review date in deny.toml | Yes | Retag |
| A `build` job fails to build, or the binary does not print `flare <version>` | Yes | Retag |
| The `formula` job fails because the tap's `scripts/formula` refuses the archive names | Yes | Retag, after the archive names in release.yml are fixed |
| The `release` job fails | See below | Re-run the failed jobs |
| The `release` job refuses the tag because its release is already published | No, the release is published | Raise the version and tag it |
| The `release` job cannot read whether the tag has a release, because the token, a rate limit, the network or GitHub failed | Unchanged: the job writes nothing | Re-run the failed jobs once GitHub answers |

The `version` job's ancestry check catches a tag placed on a commit that is
not on main, such as a branch commit tagged by mistake. The check runs from
the release.yml that the tag holds, so a person with write access who tags a
commit whose release.yml drops the check still publishes that commit. The
repository has no tag ruleset that limits who creates `v*` tags, so nothing
stops such a deliberate bypass.

`gh release create` makes a draft release, uploads the files and then
publishes the draft. When an upload fails, gh deletes the draft, so no
release exists. When the job is stopped during the upload, by its 10-minute
timeout or a cancel, the draft stays. The tap reads only published releases,
so it ignores the draft and keeps the older formula. A re-run of the
`release` job finds the draft, replaces its files and publishes it.

The `release` job replaces files only on a draft. A manual run, or a re-run
of the `release` job, on a tag whose release is already published fails with
`the release v<version> is already published, and a published release keeps
its archives; raise the version in Cargo.toml and tag the new version`, and
leaves the release, its archives and SHA256SUMS untouched. The rebuilt
archives would differ from the published ones, so the tap's formula and every
installed copy would stop matching the release. The recovery is a new
version: land a pull request that raises the version and tag that version.

The `release` job first asks gh whether the tag has a release. When gh
answers `release not found`, the job creates the release. When the read fails
for any other reason, the job prints gh's error and `could not read whether
v<version> has a release, so the job made no release; re-run the failed jobs
once GitHub answers`, and fails before it creates, uploads or publishes
anything, so a draft left by an earlier run stays the tag's only release. The
recovery is to re-run the failed jobs.

A pull request that changes the workflow, and a manual run with no tag, run
every job except `release`, so they build the archives and run the tap check
without publishing anything.
