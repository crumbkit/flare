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

1. Land a pull request that sets the workspace version in Cargo.toml and
   moves CHANGELOG.md's Unreleased entries under that version.
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

- The `version` job fails when the tag differs from the version in
  Cargo.toml. No release exists. Delete the tag, fix the version and tag
  again.
- The `advisories` job fails when a dependency has a security advisory. No
  release exists. Update the dependency, or record an ignore with a review
  date in deny.toml, and tag again.
- A `build` job fails when a target does not build, or when the binary does
  not print `flare <version>`. No release exists. Fix the cause and run the
  failed jobs again.
- The `formula` job fails when the tap's `scripts/formula` refuses the
  archive names. No release exists. Name the archives as the tap reads them.
- The `release` job can stop partway through its upload. The release then
  holds some files and may lack `SHA256SUMS`, and the tap keeps the older
  formula until `SHA256SUMS` appears. Run the `release` job again; it
  replaces the files that are already there.

A pull request that changes the workflow, and a manual run with no tag, run
every job except `release`, so they build the archives and run the tap check
without publishing anything.
