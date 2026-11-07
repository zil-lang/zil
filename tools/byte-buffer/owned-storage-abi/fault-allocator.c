/* Test-only link interception. Not a product runtime or ZIL safety proof. */
#include <stddef.h>
extern void *__real_malloc(size_t);
extern void *__real_realloc(void *, size_t);
extern void __real_free(void *);
static int attempts;
static int releases;
static int live;
static int fail_size = 13;

void *__wrap_malloc(size_t size) {
    attempts++;
    if (size == (size_t)fail_size) return NULL;
    void *next = __real_malloc(size);
    if (next) live++;
    return next;
}
void *__wrap_realloc(void *old, size_t size) {
    attempts++;
    if (size == (size_t)fail_size) return NULL;
    int was_empty = old == NULL;
    void *next = __real_realloc(old, size);
    if (next && was_empty) live++;
    return next;
}
void __wrap_free(void *old) {
    if (old) { releases++; live--; }
    __real_free(old);
}
int byte_test_attempts(void) { return attempts; }
int byte_test_releases(void) { return releases; }
int byte_test_live(void) { return live; }
