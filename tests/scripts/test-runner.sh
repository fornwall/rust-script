#!/bin/bash
set -e -u

ANY_ERROR=0

# Make sure newly built binary is first in PATH:
cargo build &> /dev/null || {
    echo "ERROR: Compilation failed"
    exit 1
}
export PATH=$PWD/target/debug/:$PATH
cd tests/scripts

for TEST_SCRIPT in *.script; do
  EXPECTED_STDOUT=${TEST_SCRIPT/.script/.expected}
  ACTUAL_STDOUT=${TEST_SCRIPT/.script/.actual-stdout}
  ACTUAL_STDERR=${TEST_SCRIPT/.script/.actual-stderr}
  echo -n "Running $TEST_SCRIPT ... "

  ./$TEST_SCRIPT > $ACTUAL_STDOUT 2> $ACTUAL_STDERR || {
    ANY_ERROR=1
    echo "Failed to run!"
  }

  if cmp -s "$EXPECTED_STDOUT" "$ACTUAL_STDOUT"; then
    echo "Ok"
  else
    ANY_ERROR=1
    echo "Failed!"
    echo "######################## Expected:"
    cat $EXPECTED_STDOUT
    echo "######################## Actual:"
    cat $ACTUAL_STDOUT
    echo "######################## Error output:"
    cat $ACTUAL_STDERR
    echo "########################"
  fi
done

exit $ANY_ERROR
