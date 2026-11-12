#!/usr/bin/env bash
# Sync the lockstep version of the JS package, the PyO3 wheel, the
# pure-Python mirror and the docs site from the Rust workspace version
# in the root Cargo.toml. Used by the release-pr workflow after
# release-plz bumps Cargo.toml; safe to run by hand too (no-op when
# everything already matches).
#
# NOTE: the version source is the root Cargo.toml [workspace.package]
# version (same as indycheck). If it is missing the script fails loudly
# instead of guessing from another file — release-plz, cargo --workspace
# and this script all need it.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -f Cargo.toml ]; then
  echo "error: no root Cargo.toml — cannot determine the workspace version" >&2
  exit 1
fi

version=$(grep -m1 '^version =' Cargo.toml | sed -E 's/version = "(.*)"/\1/')
echo "syncing lockstep version to $version"

# packages/js/package.json and website/package.json are compact
# single-line JSON ("version":"0.1.0", no space after the colon), so the
# pattern allows optional whitespace after the colon.
sed -i -E "s/\"version\": *\"[0-9]+\.[0-9]+\.[0-9]+\"/\"version\":\"$version\"/" packages/js/package.json website/package.json
sed -i -E "s/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"/version = \"$version\"/" crates/dusrelimage-py/pyproject.toml packages/python/pyproject.toml
sed -i -E "s/softwareVersion: '[0-9]+\.[0-9]+\.[0-9]+'/softwareVersion: '$version'/" website/astro.config.mjs

echo "packages/js/package.json: $(grep -o '\"version\":\"[^\"]*\"' packages/js/package.json)"
echo "crates/dusrelimage-py/pyproject.toml: $(grep -m1 '^version' crates/dusrelimage-py/pyproject.toml)"
echo "packages/python/pyproject.toml: $(grep -m1 '^version' packages/python/pyproject.toml)"
echo "website/package.json: $(grep -o '\"version\":\"[^\"]*\"' website/package.json)"
echo "website/astro.config.mjs: $(grep -m1 'softwareVersion' website/astro.config.mjs)"
