#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
static int attempts, allocations, releases;
void *__real_calloc(size_t, size_t);
void __real_free(void *);
void *__wrap_calloc(size_t count, size_t size) {
    attempts++;
    if (getenv("ZIL_TASK_AGGREGATE_FAIL")) return NULL;
    void *frame = __real_calloc(count, size);
    if (frame) allocations++;
    return frame;
}
void __wrap_free(void *frame) {
    if (frame) releases++;
    __real_free(frame);
}
static void check(void) {
    if (attempts != 1 || allocations != releases ||
        allocations != (getenv("ZIL_TASK_AGGREGATE_FAIL") ? 0 : 1)) abort();
}
__attribute__((constructor)) static void init(void) { if (atexit(check)) abort(); }
