#!/bin/sh
set -e -u
rust-script -e '1+1'
rust-script -e '1+2'
rust-script -e '1+3'
