#define _GNU_SOURCE
#include <stdint.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>

#ifndef ZIL_CHILD_PREPOLLED
#define ZIL_CHILD_PREPOLLED 0
#endif
#ifndef ZIL_ONCE_CANCEL
#define ZIL_ONCE_CANCEL 0
#endif
#ifndef ZIL_ONCE_OWNED
#define ZIL_ONCE_OWNED 0
#endif
typedef int64_t (*callback_type)(void *, int32_t);
static callback_type callback;
static void *context;
static int registered, requested, drain_calls, retired, pending, ready;
static int worker_calls, after_yield, parent_calls, attempts, releases;
static int midpoint, quiet_releases, result_releases, seen, safe_calls, safe_after;
static void *selected_frame;
static void selected_retired(void);
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
    if (ZIL_ONCE_OWNED == 2 && attempts == 3) {
        if (selected_frame) abort();
        selected_frame = pointer;
    }
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
            if (pointer == selected_frame) selected_retired();
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
static volatile uintptr_t stack_checksum;
static void clobber_old_poll_stack(void) {
    volatile uintptr_t words[8192];
    for (size_t i = 0; i < 8192; ++i) words[i] = UINTPTR_MAX - i;
    stack_checksum ^= words[0];
}
void driver_pending(void) {
    clobber_old_poll_stack();
    if (drain_calls != 1 || !context || ++pending != 1) abort();
}
void driver_ready(void) {
    if (context || ++ready != 1) abort();
}
void child_enter(void) { worker_calls++; }
void child_after_yield(void) { after_yield++; }
void parent_enter(void) { parent_calls++; }
void safe_enter(void) { safe_calls++; }
void safe_after_yield(void) { safe_after++; }
void postbody_pending(void) {
    clobber_old_poll_stack();
    if (++midpoint != 1 || parent_calls != 1) abort();
    if (registered) {
        if (drain_calls != 1 || !context || quiet_releases || seen || result_releases) abort();
    } else if (!seen || quiet_releases != 1 || result_releases != (ZIL_ONCE_OWNED != 0)) abort();
}
void quiet_retired(int32_t code) {
    int early_result = ZIL_ONCE_OWNED == 2 && ZIL_ONCE_CANCEL && registered;
    if (++quiet_releases != 1 || code != 8 ||
        (registered && (context || drain_calls != 2)) ||
        releases != (registered ? 2 : 1) + early_result ||
        region_count - releases != 1 + (ZIL_ONCE_OWNED == 2 && !early_result)) abort();
}
void result_seen(int32_t code) {
    if (++seen != 1 || code != (ZIL_ONCE_OWNED ? 17 : 8) ||
        context || quiet_releases != 1 || result_releases != (ZIL_ONCE_OWNED == 2)) abort();
}
void result_retired(int32_t code) {
    if (ZIL_ONCE_OWNED != 1 || ++result_releases != 1 || code != 17) abort();
    if (ZIL_ONCE_CANCEL && registered) {
        /* The result is newer than the remaining callable. Cancellation must
           own and destroy it before finishing that callable's pending child. */
        if (drain_calls != 1 || !context || quiet_releases || seen || releases) abort();
    } else if (context || quiet_releases != 1 || seen != 1) abort();
}
static void selected_retired(void) {
    if (ZIL_ONCE_OWNED != 2 || ++result_releases != 1) abort();
    if (ZIL_ONCE_CANCEL && registered) {
        if (drain_calls != 1 || !context || quiet_releases || seen || releases) abort();
    } else if (context || quiet_releases != 1 || seen) abort();
}
static void check(void) {
    int failure = fail_at();
    int expected = ZIL_ONCE_OWNED == 2
        ? (failure == 2 ? 2 : (failure == 3 ? 3 : (failure == 4 ? 5 : 4)))
        : (failure == 2 ? 2 : (failure == 3 ? 4 : 3));
    int active = failure != 1;
    int entered = failure != 2 && !(ZIL_ONCE_OWNED == 2 && failure == 3);
    int completed = entered && (!ZIL_ONCE_CANCEL || !active);
    if (failure < 0 || failure > (ZIL_ONCE_OWNED == 2 ? 4 : 3) || attempts != expected ||
        region_count != expected - (failure != 0) || releases != region_count ||
        registered != active || requested != active || retired != active ||
        drain_calls != 2 * active || callback || context || after_yield ||
        worker_calls != (ZIL_CHILD_PREPOLLED && failure != 2) || parent_calls != entered ||
        safe_calls != (ZIL_CHILD_PREPOLLED && ZIL_ONCE_OWNED == 2 && entered) || safe_after ||
        midpoint != (ZIL_ONCE_CANCEL && entered) || quiet_releases != entered ||
        seen != completed || result_releases != (ZIL_ONCE_OWNED != 0 && entered) ||
        pending != (!ZIL_ONCE_CANCEL && active && entered) ||
        ready != (!ZIL_ONCE_CANCEL && entered)) abort();
}
__attribute__((constructor)) static void initialize(void) { if (atexit(check)) abort(); }
