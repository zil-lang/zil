#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

void *__real_calloc(size_t, size_t);
void __real_free(void *);

static void *frames[2];
static unsigned allocations, frees, observations;

void *__wrap_calloc(size_t count, size_t size) {
    void *frame = __real_calloc(count, size);
    if (frame != NULL) {
        if (allocations == 2) abort();
        frames[allocations++] = frame;
    }
    return frame;
}

void __wrap_free(void *value) {
    if (value != NULL) {
        if (frees == 2 || value != frames[1 - frees]) abort();
        ++frees;
    }
    __real_free(value);
}

void child_observe(int32_t code) {
    if (allocations != 2 || observations >= 2 ||
        code != (int32_t)('A' + observations) || frees != observations) abort();
    ++observations;
    putchar(code);
}

int32_t child_close_complete(void) {
    if (allocations != 2 || frees != 2 || observations != 2) return 70;
    puts(" selected child closed between its neighboring fields");
    return 0;
}
