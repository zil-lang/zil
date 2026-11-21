#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void __real_free(void *);
struct Block { void *pointer; size_t size; };
static struct Block blocks[64];
static unsigned requests, acquired, released, active, moved, bad;
static void report(void) {
    printf("|%u:%u:%u:%u:%u:%u", requests, acquired, released, active, moved, bad);
    if (active || bad) { fflush(stdout); _Exit(97); }
}
__attribute__((constructor)) static void observe(void) { atexit(report); }
void *__wrap_realloc(void *pointer, size_t size) {
    unsigned slot = 64;
    for (unsigned i = 0; i < 64; i++) if (pointer && blocks[i].pointer == pointer) slot = i;
    if (pointer && slot == 64) { bad++; abort(); }
    requests++;
    const char *failure = getenv("ZIL_ANONYMOUS_FAIL_AT");
    if (failure && strtoul(failure, NULL, 10) == requests) return NULL;
    if (!size) { bad++; abort(); }
    // Allocate before retiring the old block: each successful growth really
    // moves the containing element bytes, while child heaps keep their identity.
    void *next = malloc(size);
    if (!next) abort();
    if (pointer) {
        memcpy(next, pointer, blocks[slot].size < size ? blocks[slot].size : size);
        memset(pointer, 0xa5, blocks[slot].size);
        __real_free(pointer);
        moved++;
    } else {
        for (unsigned i = 0; i < 64; i++) if (!blocks[i].pointer) { slot = i; break; }
        if (slot == 64) abort();
        acquired++; active++;
    }
    blocks[slot] = (struct Block){next, size};
    return next;
}
void __wrap_free(void *pointer) {
    if (pointer) {
        unsigned found = 0;
        for (unsigned i = 0; i < 64; i++) if (blocks[i].pointer == pointer) {
            memset(pointer, 0xa5, blocks[i].size);
            blocks[i] = (struct Block){NULL, 0};
            released++; active--; found++;
        }
        if (found != 1) { bad++; abort(); }
    }
    __real_free(pointer);
}
