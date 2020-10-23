#!/bin/sh
set -e -u

echo "First:"
echo "line1\nline2" | rust-script --loop \
    "let mut n=0; move |l| {n+=1; println!(\"{:>6}: {}\",n,l.trim_right())}"

echo "Second:"
echo asdf
echo "line1\nline2" | rust-script --count --loop \
    "|l,n| println!(\"{:>6}: {}\", n, l.trim_right())"
