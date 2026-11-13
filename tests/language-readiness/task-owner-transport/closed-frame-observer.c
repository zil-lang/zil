#define _GNU_SOURCE
#include <stdint.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>

typedef int64_t (*callback_type)(void *, int32_t);
static callback_type callback;
static void *context;
static int registered, requested, drained, attempts, releases, observed;
static struct { void *pointer; size_t bytes; int live; } regions[8];
static int allocations;

static int fail_at(void) {
    const char *value = getenv("ZIL_CLOSED_FAIL_AT");
    return value ? atoi(value) : 0;
}
void *__wrap_calloc(size_t count, size_t size) {
    if (++attempts == fail_at()) return NULL;
    if (!count || !size || size > SIZE_MAX / count || allocations == 8) abort();
    size_t page = (size_t)sysconf(_SC_PAGESIZE);
    size_t bytes = count * size;
    if (bytes > SIZE_MAX - page) abort();
    bytes = ((bytes + page - 1) / page) * page;
    void *pointer = mmap(NULL, bytes, PROT_READ | PROT_WRITE,
                         MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (pointer == MAP_FAILED) abort();
    regions[allocations].pointer = pointer;
    regions[allocations].bytes = bytes;
    regions[allocations++].live = 1;
    return pointer;
}
void __real_free(void *);
void __wrap_free(void *pointer) {
    if (!pointer) return;
    for (int i = 0; i < allocations; ++i) {
        if (regions[i].pointer != pointer) continue;
        if (!regions[i].live) abort();
        regions[i].live = 0;
        releases++;
        if (mprotect(pointer, regions[i].bytes, PROT_NONE)) abort();
        return;
    }
    __real_free(pointer);
}
void *reg(callback_type cb, void *ctx) {
    if (registered++ || !cb || !ctx) abort();
    callback = cb;
    context = ctx;
    return &registered;
}
void request(void *token) {
    if (token != &registered || requested++ || !context) abort();
}
int32_t drain(void *token) {
    if (token != &registered || requested != 1 || !context) abort();
    if (++drained == 1) {
        /* A real holder still has the raw pointer before entering trampoline.
           Closing must reject the callback without running the user's body. */
        if ((uint64_t)callback(context, 7) != (UINT64_C(1) << 32)) abort();
        return 0;
    }
    if (drained != 2) abort();
    callback = NULL;
    context = NULL;
    return 1;
}
void ready_observed(void) {
    /* A later-created lexical Witness drops before the callable. The Task
       must already be Closed here: env destruction cannot hide extra drain. */
    if (observed++ || context || (registered && (requested != 1 || drained != 2)) ||
        releases != allocations - 1) abort();
}
static void check(void) {
    if (attempts != 2 || allocations != releases ||
        allocations != (fail_at() ? 1 : 2) ||
        observed != (fail_at() == 2 ? 0 : 1) || context ||
        (registered && (requested != 1 || drained != 2))) abort();
}
__attribute__((constructor)) static void init(void) {
    if (atexit(check)) abort();
}
