// Fault injection observes the generated allocator ABI; it implements no task.
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
