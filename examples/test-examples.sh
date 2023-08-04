#!/bin/sh
set -e -u

assert_equals() {
    if [ "$1" != "$2" ]; then
      echo "Invalid output: Expected '$1', was '$2'"
      exit 1
    fi
}

assert_equals "result: 3" "$(just -f justfile/Justfile)"
assert_equals "hello, rust" "$(./hello.ers)"
assert_equals "hello, rust" "$(./hello-without-main.ers)"

HYPERFINE_OUTPUT=$(rust-script --wrapper "hyperfine --runs 99" fib.ers)

case "$HYPERFINE_OUTPUT" in
  *"99 runs"*)
    ;;
  *)
    echo "Hyperfine output: $HYPERFINE_OUTPUT"
    exit 1
    ;;
esac
