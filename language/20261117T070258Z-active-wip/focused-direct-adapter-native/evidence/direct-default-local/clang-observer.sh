#!/bin/sh
set -eu
link=1
input=0
for arg in "$@"; do
case "$arg" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac
done
if [ "$link" = 1 ] && [ "$input" = 1 ]; then
exec /workspace/.zil-env/llvm-23.1.4/bin/clang "$@" -x none /workspace/zil-alpha-candidate/out/runs/alpha-resume-checked-cleanup-routes-exact-return-ports-and-owned-products/existing-direct-cleanup-native/direct-default-local/observer.o -Wl,--wrap=calloc -Wl,--wrap=free
fi
exec /workspace/.zil-env/llvm-23.1.4/bin/clang "$@"
