#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
static int attempts, allocations, releases, quiet_calls, quiet_releases;
#ifndef ZIL_TASK_EXPECTED_QUIET
#define ZIL_TASK_EXPECTED_QUIET 0
#endif
#ifndef ZIL_TASK_EXPECTED_QUIET_CALLS
#define ZIL_TASK_EXPECTED_QUIET_CALLS 0
#endif
void quiet_taken(void) { quiet_calls++; }
#ifndef ZIL_TASK_DIRECT_CHILD
#define ZIL_TASK_DIRECT_CHILD 0
#endif
static int fail_at(void);
void quiet_retired(int32_t code) {
    /* Only a failed parent constructor explicitly invokes the returned F.
       Cleanup in either worker representation must never run its user body. */
    int called = fail_at() == 2 && !ZIL_TASK_DIRECT_CHILD;
    if (code != (called ? 8 : 7)) abort();
    quiet_releases++;
}
#ifndef ZIL_TASK_EXPECTED_FRAMES
#define ZIL_TASK_EXPECTED_FRAMES 1
#endif
static int fail_at(void) {
    if (getenv("ZIL_TASK_AGGREGATE_FAIL")) return 1;
    const char *selected = getenv("ZIL_TASK_AGGREGATE_FAIL_AT");
    return selected ? atoi(selected) : 0;
}
void *__real_calloc(size_t, size_t);
void __real_free(void *);
void *__wrap_calloc(size_t count, size_t size) {
    attempts++;
    if (attempts == fail_at()) return NULL;
    void *frame = __real_calloc(count, size);
    if (frame) allocations++;
    return frame;
}
void __wrap_free(void *frame) {
    if (frame) releases++;
    __real_free(frame);
}
static void check(void) {
    int failed = fail_at();
    if (quiet_releases != ZIL_TASK_EXPECTED_QUIET ||
        quiet_calls != (failed ? 0 : ZIL_TASK_EXPECTED_QUIET_CALLS)) abort();
    if (failed < 0 || failed > ZIL_TASK_EXPECTED_FRAMES ||
        attempts != (failed ? failed : ZIL_TASK_EXPECTED_FRAMES) ||
        allocations != releases ||
        allocations != (failed ? failed - 1 : ZIL_TASK_EXPECTED_FRAMES)) abort();
}
__attribute__((constructor)) static void init(void) { if (atexit(check)) abort(); }
