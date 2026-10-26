/* Untimed application allocation observer. No replacement allocator. */
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

void *__real_malloc(size_t);
void *__real_calloc(size_t, size_t);
void *__real_realloc(void *, size_t);
void __real_free(void *);

struct allocation { void *pointer; size_t bytes, id; };
static struct allocation active[64];
static size_t next_id, attempts, fail_at, live_bytes, peak_bytes;
static size_t allocated, released, unknown_frees;
static int enabled;

static struct allocation *find(void *pointer) {
    for (size_t i = 0; i < 64; ++i)
        if (active[i].pointer == pointer && pointer) return &active[i];
    return NULL;
}
static struct allocation *empty_slot(void) {
    for (size_t i = 0; i < 64; ++i)
        if (!active[i].pointer) return &active[i];
    fputs("observer capacity exhausted\n", stderr);
    abort();
}
static void record_reserve(struct allocation *old, void *pointer,
                           size_t bytes, int success) {
    size_t old_bytes = old ? old->bytes : 0;
    size_t id = old ? old->id : 0;
    if (success) {
        if (!old) { old = empty_slot(); id = ++next_id; ++allocated; }
        old->pointer = pointer; old->bytes = bytes; old->id = id;
        live_bytes = live_bytes - old_bytes + bytes;
        if (live_bytes > peak_bytes) peak_bytes = live_bytes;
    }
    fprintf(stderr, "{\"op\":\"reserve\",\"attempt\":%zu,\"id\":%zu,"
            "\"old\":%zu,\"requested\":%zu,\"success\":%s}\n",
            attempts, id, old_bytes, bytes, success ? "true" : "false");
}
void *__wrap_malloc(size_t bytes) {
    if (!enabled) return __real_malloc(bytes);
    ++attempts;
    void *result = attempts == fail_at ? NULL : __real_malloc(bytes);
    record_reserve(NULL, result, bytes, result != NULL);
    return result;
}
void *__wrap_calloc(size_t count, size_t width) {
    if (!enabled) return __real_calloc(count, width);
    if (width && count > SIZE_MAX / width) abort();
    ++attempts;
    void *result = attempts == fail_at ? NULL : __real_calloc(count, width);
    record_reserve(NULL, result, count * width, result != NULL);
    return result;
}
void *__wrap_realloc(void *pointer, size_t bytes) {
    if (!enabled) return __real_realloc(pointer, bytes);
    if (!bytes) abort(); /* The registered consumers never request size zero. */
    struct allocation *old = find(pointer);
    if (pointer && !old) abort();
    ++attempts;
    void *result = attempts == fail_at ? NULL : __real_realloc(pointer, bytes);
    record_reserve(old, result, bytes, result != NULL);
    return result;
}
void __wrap_free(void *pointer) {
    if (enabled && pointer) {
        struct allocation *old = find(pointer);
        if (old) {
            fprintf(stderr, "{\"op\":\"release\",\"id\":%zu,\"bytes\":%zu}\n",
                    old->id, old->bytes);
            live_bytes -= old->bytes; old->pointer = NULL; ++released;
        } else {
            ++unknown_frees;
            fputs("{\"op\":\"unknown_free\"}\n", stderr);
        }
    }
    __real_free(pointer);
}
void trace_start(void) {
    setvbuf(stderr, NULL, _IONBF, 0);
    const char *failure = getenv("ZIL_PARITY_FAIL_AT");
    fail_at = failure ? strtoull(failure, NULL, 10) : 0;
    enabled = 1;
    fputs("{\"op\":\"begin\"}\n", stderr);
}
void trace_end(void) {
    enabled = 0;
    fprintf(stderr, "{\"op\":\"end\",\"attempts\":%zu,\"live\":%zu,"
            "\"peak\":%zu,\"allocations\":%zu,\"releases\":%zu,"
            "\"unknown_frees\":%zu}\n", attempts, live_bytes, peak_bytes,
            allocated, released, unknown_frees);
}
