#!/usr/bin/env bash
#
# Exercises next-version.sh against throwaway git repositories.

set -euo pipefail

readonly SCRIPT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/next-version.sh"

failures=0

commit() {
  git -c user.name=test -c user.email=test@example.com -c commit.gpgsign=false \
    commit --quiet --allow-empty -m "$1"
}

tag() {
  git -c tag.gpgsign=false tag "$1"
}

# Runs the body in a fresh repository and compares what the script prints.
expect() {
  local description="$1"
  local expected="$2"
  local body="$3"

  local repository
  repository="$(mktemp -d)"

  local actual
  actual="$(
    cd "$repository"
    git init --quiet --initial-branch=main
    eval "$body"
    bash "$SCRIPT"
  )"
  rm -rf "$repository"

  if [[ "$actual" == "$expected" ]]; then
    echo "ok   - $description"
  else
    echo "FAIL - $description: expected '$expected', got '$actual'"
    failures=$((failures + 1))
  fi
}

expect "no tag starts at the initial version" "1.0.0" \
  'commit "feat: first"'

expect "no commits since the tag prints nothing" "" \
  'commit "feat: first"; tag v1.2.3'

expect "fix is a patch" "1.2.4" \
  'commit "x"; tag v1.2.3; commit "fix: bug"'

expect "uncategorised commit is a patch" "1.2.4" \
  'commit "x"; tag v1.2.3; commit "update readme"'

expect "feat is a minor and resets patch" "1.3.0" \
  'commit "x"; tag v1.2.3; commit "feat: new thing"'

expect "scoped feat is a minor" "1.3.0" \
  'commit "x"; tag v1.2.3; commit "feat(pads): new thing"'

expect "subject type is case insensitive" "1.3.0" \
  'commit "x"; tag v1.2.3; commit "Feat: new thing"'

expect "feature word alone is not a feat" "1.2.4" \
  'commit "x"; tag v1.2.3; commit "feature flag cleanup"'

expect "bang is a major and resets the rest" "2.0.0" \
  'commit "x"; tag v1.2.3; commit "feat!: redo"'

expect "scoped bang is a major" "2.0.0" \
  'commit "x"; tag v1.2.3; commit "fix(audio)!: redo"'

expect "breaking footer is a major" "2.0.0" \
  'commit "x"; tag v1.2.3; commit "$(printf "fix: redo\n\nBREAKING CHANGE: new format")"'

expect "lowercase footer is not breaking" "1.2.4" \
  'commit "x"; tag v1.2.3; commit "$(printf "fix: redo\n\nbreaking change: no")"'

expect "highest bump wins whatever the order" "2.0.0" \
  'commit "x"; tag v1.2.3; commit "feat!: a"; commit "fix: b"; commit "feat: c"'

expect "minor beats older patches" "1.3.0" \
  'commit "x"; tag v1.2.3; commit "fix: a"; commit "feat: b"; commit "fix: c"'

expect "latest tag is compared numerically" "1.10.1" \
  'commit "x"; tag v1.9.0; commit "y"; tag v1.10.0; commit "fix: z"'

expect "tags that are not versions are ignored" "1.2.4" \
  'commit "x"; tag v1.2.3; commit "y"; tag nightly; tag v2-beta; commit "fix: z"'

expect "commits before the tag do not count" "1.0.1" \
  'commit "feat!: old"; tag v1.0.0; commit "fix: new"'

if [[ "$failures" -gt 0 ]]; then
  echo "$failures test(s) failed"
  exit 1
fi
