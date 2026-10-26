#!/usr/bin/env bash
# Keep the original supervisor protocol intact. Some restricted environments
# cannot start Node with libuv socketpair stdio; these are real POSIX pipes.
# "$@" remains an argument vector. pipefail preserves a failing worker status.
set -o pipefail
/usr/bin/cat | "$@" 2> >(/usr/bin/cat >&2) | /usr/bin/cat
