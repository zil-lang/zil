#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>

void *__real_realloc(void *, size_t);
void __real_free(void *);
static unsigned requests, acquired, released, active;
static void *blocks[16];
static void report(void) {
    printf("|%u:%u:%u:%u", requests, acquired, released, active);
}
__attribute__((constructor)) static void observe(void) { atexit(report); }
void *__wrap_realloc(void *pointer, size_t size) {
    const char *at = getenv("ZIL_ITER_FAIL_AT");
    unsigned slot = 16;
    for (unsigned i = 0; i < 16; i++) if (blocks[i] == pointer && pointer) slot = i;
    requests++;
    if (at && strtoul(at, NULL, 10) == requests) return NULL;
    void *result = __real_realloc(pointer, size);
    if (result) {
        if (slot == 16) {
            for (unsigned i = 0; i < 16; i++) if (!blocks[i]) { slot = i; break; }
            if (slot == 16) abort();
            acquired++; active++;
        }
        blocks[slot] = result;
    }
    return result;
}
void __wrap_free(void *pointer) {
    if (pointer) {
        unsigned found = 0;
        for (unsigned i = 0; i < 16; i++) if (blocks[i] == pointer) {
            blocks[i] = NULL; released++; active--; found++;
        }
        if (found != 1) abort();
    }
    __real_free(pointer);
}
