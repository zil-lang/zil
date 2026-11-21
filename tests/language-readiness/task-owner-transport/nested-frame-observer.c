#define _GNU_SOURCE
#include <stdint.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>

#ifndef ZIL_CHILD_PREPOLLED
#define ZIL_CHILD_PREPOLLED 0
#endif
#ifndef ZIL_CHILD_MID_DROP
#define ZIL_CHILD_MID_DROP 0
#endif
typedef int64_t (*callback_type)(void *, int32_t);
static callback_type callback;
static void *context;
static int registered, requested, drain_calls, retired, pending, ready;
static int worker_calls, after_yield, parent_calls, attempts, releases;
static int midpoint, quiet_releases;
static struct { void *pointer; size_t size; int live; } regions[8];
static int region_count;
static int fail_at(void) {
    const char *value = getenv("ZIL_CHILD_FAIL_AT");
    return value ? atoi(value) : 0;
}
void *__wrap_calloc(size_t count, size_t size) {
    if (++attempts == fail_at()) return NULL;
    if (!count || !size || size > SIZE_MAX / count || region_count == 8) abort();
    size_t page = (size_t)sysconf(_SC_PAGESIZE);
    size_t bytes = count * size;
    if (bytes > SIZE_MAX - page) abort();
    bytes = ((bytes + page - 1) / page) * page;
    void *pointer = mmap(NULL, bytes, PROT_READ | PROT_WRITE,
                         MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (pointer == MAP_FAILED) abort();
    regions[region_count].pointer = pointer;
    regions[region_count].size = bytes;
    regions[region_count++].live = 1;
    return pointer;
}
void __real_free(void *);
void __wrap_free(void *pointer) {
    if (!pointer) return;
    for (int i = 0; i < region_count; ++i) {
        if (regions[i].pointer == pointer) {
            if (!regions[i].live) abort();
            regions[i].live = 0;
            releases++;
            /* Quarantine the whole allocation. Any later marker/head write,
               callback or second cleanup is a deterministic memory fault. */
            if (mprotect(pointer, regions[i].size, PROT_NONE)) abort();
            return;
        }
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
    if (++drain_calls == 1) {
        /* The provider still holds a real raw callback/context while Closing.
           This callback must not reach the user's observable callback body. */
        if ((uint64_t)callback(context, 7) != (UINT64_C(1) << 32)) abort();
        return 0;
    }
    if (drain_calls != 2) abort();
    callback = NULL;
    context = NULL;
    retired++;
    return 1;
}
void driver_pending(void) {
    if (drain_calls != 1 || !context || ++pending != 1) abort();
}
void driver_ready(void) {
    if (context || ++ready != 1) abort();
}
void child_enter(void) { worker_calls++; }
void child_after_yield(void) { after_yield++; }
void parent_enter(void) { parent_calls++; }
void child_drop_pending(void) {
    if (++midpoint != 1 || parent_calls != 1 ||
        (registered && (drain_calls != 1 || !context))) abort();
}
void quiet_retired(int32_t code) {
    /* Capture order is Task then Quiet. The complete child must already be
       destroyed, while a successfully allocated parent still owns its env. */
    if (++quiet_releases != 1 || code != (fail_at() == 3 ? 8 : 7) ||
        (registered && (context || drain_calls != 2)) ||
        releases != (registered ? 2 : 1) ||
        region_count - releases != (fail_at() == 3 ? 0 : 1)) abort();
}
static void check(void) {
    int failure = fail_at();
    int expected = failure == 2 ? 2 : 3;
    int active = failure != 1;
    if (failure < 0 || failure > 3 || attempts != expected ||
        region_count != expected - (failure != 0) || releases != region_count ||
        registered != active || requested != active || retired != active ||
        drain_calls != 2 * active || callback || context || after_yield ||
        worker_calls != (ZIL_CHILD_PREPOLLED && failure != 2) ||
        parent_calls != (ZIL_CHILD_MID_DROP && failure != 2 && failure != 3) ||
        midpoint != (ZIL_CHILD_MID_DROP && failure != 2 && failure != 3) ||
        quiet_releases != (failure != 2) ||
        pending != (active && failure != 2 && failure != 3 && !ZIL_CHILD_MID_DROP) ||
        ready != (failure != 2 && failure != 3)) abort();
}
__attribute__((constructor)) static void initialize(void) { if (atexit(check)) abort(); }
