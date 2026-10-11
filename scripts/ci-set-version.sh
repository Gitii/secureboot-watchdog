#!/usr/bin/env bash
# Stamp debian/changelog with the version of this build, for the Ubuntu
# release the script runs on:
#   tag vX.Y.Z   -> X.Y.Z~ubuntu24.04   (the tag must match debian/changelog)
#   anything else -> X.Y.Z+git<date>.<sha>~ubuntu24.04
# Used by the Build workflow; it edits debian/changelog in place, so run it
# in a throwaway checkout. Prints the version it set.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
# shellcheck source=/dev/null
. /etc/os-release

export DEBEMAIL="${DEBEMAIL:-gitii@users.noreply.github.com}"
export DEBFULLNAME="${DEBFULLNAME:-Gitii}"

upstream="$(dpkg-parsechangelog -S Version)"
ref="${GITHUB_REF:-$(git symbolic-ref -q HEAD || true)}"
# On pull requests GITHUB_SHA is the merge commit; CI passes the head in PR_HEAD_SHA.
sha="${PR_HEAD_SHA:-${GITHUB_SHA:-$(git rev-parse HEAD)}}"

if [[ "$ref" == refs/tags/v* ]]; then
    tag="${ref#refs/tags/v}"
    if [[ "$tag" != "$upstream" ]]; then
        echo "tag v$tag does not match debian/changelog ($upstream)" >&2
        exit 1
    fi
    version="${upstream}~ubuntu${VERSION_ID}"
else
    version="${upstream}+git$(date -u +%Y%m%d).${sha::7}~ubuntu${VERSION_ID}"
fi

# -b: "X.Y.Z~ubuntu…" sorts below "X.Y.Z", which dch refuses without it.
dch -b -v "$version" -D "$VERSION_CODENAME" --force-distribution \
    "Automated build for Ubuntu ${VERSION_ID}." >&2
echo "$version"
