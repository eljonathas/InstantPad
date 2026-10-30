#!/usr/bin/env bash
#
# Prints the next release version, worked out from the Conventional Commits
# made since the latest vX.Y.Z tag reachable from HEAD:
#
#   "BREAKING CHANGE:" footer or "type!:" subject -> major
#   "feat:" subject                               -> minor
#   anything else                                 -> patch
#
# With no tag yet it prints the initial version. With no commits since the
# latest tag it prints nothing, so rerunning a released commit does nothing.

set -euo pipefail

readonly INITIAL_VERSION="1.0.0"

find_latest_tag() {
  git tag --merged HEAD --list 'v*' --sort=-version:refname |
    grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' |
    head -n 1 || true
}

classify_commit() {
  local message="$1"
  local subject="${message%%$'\n'*}"
  local lowered_subject="${subject,,}"

  local breaking_subject='^[a-z]+(\([^)]*\))?!:'
  local feature_subject='^feat(\([^)]*\))?:'
  local breaking_footer=$'(^|\n)BREAKING[ -]CHANGE:'

  if [[ "$lowered_subject" =~ $breaking_subject || "$message" =~ $breaking_footer ]]; then
    echo "major"
  elif [[ "$lowered_subject" =~ $feature_subject ]]; then
    echo "minor"
  else
    echo "patch"
  fi
}

highest_bump() {
  local commit_range="$1"
  local bump="patch"
  local message commit_bump

  while IFS= read -r -d '' message; do
    commit_bump="$(classify_commit "$message")"
    if [[ "$commit_bump" == "major" ]]; then
      echo "major"
      return
    fi
    if [[ "$commit_bump" == "minor" ]]; then
      bump="minor"
    fi
  done < <(git log -z --format=%B "$commit_range")

  echo "$bump"
}

increment_version() {
  local version="$1"
  local bump="$2"
  local major minor patch
  IFS=. read -r major minor patch <<<"$version"

  case "$bump" in
    major) echo "$((major + 1)).0.0" ;;
    minor) echo "$major.$((minor + 1)).0" ;;
    patch) echo "$major.$minor.$((patch + 1))" ;;
  esac
}

main() {
  local tag
  tag="$(find_latest_tag)"

  if [[ -z "$tag" ]]; then
    echo "$INITIAL_VERSION"
    return
  fi

  local commit_range="$tag..HEAD"
  if [[ "$(git rev-list --count "$commit_range")" -eq 0 ]]; then
    return
  fi

  increment_version "${tag#v}" "$(highest_bump "$commit_range")"
}

main
