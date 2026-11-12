#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
static unsigned attempts, allocations, frees;
static void *frame;
void *__real_calloc(size_t,size_t);
void __real_free(void*);
static void check(void) {
    assert(attempts == 1);
    if (getenv("ZIL_TASK_ALLOC_FAIL")) assert(!allocations && !frees);
    else assert(allocations == 1 && frees == 1);
    puts("original borrowed input retained through constructor outcome");
}
void *__wrap_calloc(size_t n,size_t s) {
    assert(attempts++ == 0);
    assert(atexit(check) == 0);
    if (getenv("ZIL_TASK_ALLOC_FAIL")) return NULL;
    frame = __real_calloc(n,s);
    assert(frame);
    allocations++;
    return frame;
}
void __wrap_free(void *p) {
    assert(p == frame && frees++ == 0);
    __real_free(p);
}
