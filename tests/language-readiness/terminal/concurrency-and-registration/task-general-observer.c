// Observes actual generated allocations and source-visible poll branches.
// This file supplies no task frame, state machine or cleanup implementation.
#include <stddef.h>
#include <stdlib.h>
#include <unistd.h>
void *__real_calloc(size_t count, size_t size);
void __real_free(void *pointer);
void *__wrap_calloc(size_t count, size_t size) {
    (void)write(1, "A", 1);
    if (getenv("ZIL343_TASK_ALLOC_FAIL")) return NULL;
    return __real_calloc(count, size);
}
void __wrap_free(void *pointer) {
    if (pointer) (void)write(1, "F", 1);
    __real_free(pointer);
}
int task_general_observe(int value) {
    const unsigned char byte = (unsigned char)value;
    return write(1, &byte, 1) == 1 ? value : -1;
}
