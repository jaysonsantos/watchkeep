#!/usr/bin/env bash
# Tag a release. The script reads the Conventional Commits since the last tag,
# computes the next version with git-cliff, writes that version into Cargo.toml
# and package.json, regenerates CHANGELOG.md, then commits and tags:
#
#   scripts/release.sh          # the version comes from the commits
#   scripts/release.sh 0.3.0    # the version comes from the argument
#
# The script pushes nothing. `git push --follow-tags` sends the tag and starts
# the release workflow (.github/workflows/release.yml).
#
# Needs git-cliff, from the nix flake or from `cargo install git-cliff`.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(dirname "$script_dir")"

cargo_manifest="Cargo.toml"
cargo_lock="Cargo.lock"
package_manifest="package.json"
changelog="CHANGELOG.md"
version_pattern='^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$'

cd "$repo_dir"

command -v git-cliff >/dev/null || {
  echo "git-cliff is missing: enter the nix shell or run 'cargo install git-cliff'" >&2
  exit 1
}

# Untracked files stay out of the release, so only the tracked ones matter here.
[[ -z "$(git status --porcelain --untracked-files=no)" ]] || {
  echo "the tracked files have changes: commit or stash them first" >&2
  exit 1
}

version="${1:-$(git cliff --bumped-version)}"
version="${version#v}"
tag="v$version"

[[ "$version" =~ $version_pattern ]] || {
  echo "'$version' is not a version number" >&2
  exit 1
}

! git rev-parse -q --verify "refs/tags/$tag" >/dev/null || {
  echo "the tag $tag exists" >&2
  exit 1
}

# The version of the workspace; each crate reads it with `version.workspace = true`.
sed -i "/^\[workspace\.package\]/,/^\[/ s/^version = \".*\"\$/version = \"$version\"/" "$cargo_manifest"
# The first "version" key of package.json, the one next to "name".
sed -i "0,/\"version\":/ s/\"version\": \"[^\"]*\"/\"version\": \"$version\"/" "$package_manifest"
# The lock file holds the version of each workspace member.
cargo update --workspace --quiet

git cliff --tag "$tag" --output "$changelog"
# The notes of this release alone: no header, no footer, and no version heading,
# because the name of the release carries the version already.
notes="$(git cliff --tag "$tag" --unreleased --strip all | sed "/^## \[/d")"

git add -- "$cargo_manifest" "$cargo_lock" "$package_manifest" "$changelog"
git commit --quiet --message "chore(release): $tag"
# `--cleanup=whitespace` keeps the "###" headings of the notes, which the default mode strips.
git tag --annotate --cleanup=whitespace "$tag" --message "$tag" --message "$notes"

echo "$tag is ready. Push it with: git push --follow-tags"
