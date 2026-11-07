#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

extern void *__real_realloc(void *, size_t);
extern void *__real_calloc(size_t, size_t);
extern void __real_free(void *);
static int attempts;
static int allocations;
static int releases;
static int blocked(void) {
    const char *at = getenv("ZIL_POD_ALLOC_FAIL_AT");
    ++attempts;
    return at != NULL && attempts == atoi(at);
}
void *__wrap_realloc(void *old, size_t size) {
    if (blocked()) return NULL;
    void *value = __real_realloc(old, size);
    if (value != NULL && old == NULL) ++allocations;
    return value;
}
void *__wrap_calloc(size_t count, size_t size) {
    if (blocked()) return NULL;
    void *value = __real_calloc(count, size);
    if (value != NULL) ++allocations;
    return value;
}
void __wrap_free(void *value) {
    if (value != NULL) ++releases;
    __real_free(value);
}
__attribute__((destructor)) static void report(void) {
    printf("%d:%d:%d\n", attempts, allocations, releases);
}
