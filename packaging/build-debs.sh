#!/usr/bin/env bash
# Build the secureboot-watchdog .deb for Ubuntu 24.04 (noble) and 26.04
# (resolute) in clean containers, from the committed tree only (git archive HEAD).
#
#   packaging/build-debs.sh [noble|resolute|all]      (default: all)
#
# Output: dist/debs/<codename>/.
set -Eeuo pipefail

NAME=secureboot-watchdog

# Runs inside the container, in a copy of the source; $CODENAME is set.
# No -d: dpkg-buildpackage checks debian/control's Build-Depends.
build() {
  local ver
  ver=$(dpkg-parsechangelog -S Version)
  sed -i "1s/(${ver})/(${ver}~${CODENAME}1)/" debian/changelog
  dpkg-buildpackage -us -uc -b
  cp ../*.deb /out/
}

# ---- common part: identical in every repo that carries this script ---------

ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
declare -A BASE_IMAGE=( [noble]=ubuntu:24.04 [resolute]=ubuntu:26.04 )

die()  { printf '\nERROR: %s\n' "$*" >&2; exit 1; }
step() { printf '\n==> %s\n' "$*" >&2; }

case "${1:-all}" in
  all) targets=(noble resolute) ;;
  noble|resolute) targets=("$1") ;;
  *) echo "usage: $0 [noble|resolute|all]" >&2; exit 2 ;;
esac

# Use docker directly when this user may; otherwise go through sudo.
if docker info >/dev/null 2>&1; then DOCKER=(docker); else DOCKER=(sudo docker); fi

# Packages must match a commit: sources and packaging/Dockerfile.build come
# from `git archive HEAD`. Only build() above is taken from this file as it is.
if [ -n "$(git -C "$ROOT" status --porcelain)" ]; then
  echo "note: uncommitted or untracked changes are NOT included; building $(git -C "$ROOT" rev-parse --short HEAD)" >&2
fi
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
mkdir "$work/src" "$work/ctx"
git -C "$ROOT" archive HEAD | tar -x -C "$work/src"

# Docker build context: the Dockerfile, plus debian/control when present so
# the image can install Build-Depends from the single source of truth.
cp "$work/src/packaging/Dockerfile.build" "$work/ctx/Dockerfile"
[ -f "$work/src/debian/control" ] && cp "$work/src/debian/control" "$work/ctx/control"

for codename in "${targets[@]}"; do
  image="${NAME}-debbuild:${codename}"
  out="$ROOT/dist/debs/$codename"
  mkdir -p "$out"
  rm -f "$out"/*.deb        # never keep, count or install a stale package
  step "$NAME: building image $image (${BASE_IMAGE[$codename]})"
  "${DOCKER[@]}" build --progress=plain --build-arg "UBUNTU_VERSION=${BASE_IMAGE[$codename]#ubuntu:}" \
    -t "$image" "$work/ctx" >"$work/image-$codename.log" 2>&1 \
    || { tail -40 "$work/image-$codename.log" >&2; die "image build failed for $codename"; }
  step "$NAME: building packages for $codename"
  "${DOCKER[@]}" run --rm \
    -v "$work/src":/src:ro -v "$out":/out \
    -e CODENAME="$codename" -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    "$image" bash -Eeuo pipefail -c "
      mkdir -p /build && cp -a /src/. /build/ && cd /build
      $(declare -f build)
      build
      chown -R \"\$HOST_UID:\$HOST_GID\" /out
    " || die "$NAME failed for $codename"
  step "$NAME/$codename: $(find "$out" -maxdepth 1 -name '*.deb' | wc -l) package(s) in dist/debs/$codename"
done
