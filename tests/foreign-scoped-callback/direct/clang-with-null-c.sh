#!/bin/sh
case_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 1
exec clang "$@" -x c "$case_dir/driver-null.c"
