#include <stddef.h>
#include <stdlib.h>
#include <unistd.h>

void *__real_calloc(size_t count, size_t size);
void __real_free(void *ptr);

static void *owned_frame;
static unsigned attempts;
static unsigned releases;

void roam_task_emit(int byte) {
    unsigned char event = (unsigned char)byte;
    if (write(1, &event, 1) != 1) abort();
}

void *__wrap_calloc(size_t count, size_t size) {
    if (++attempts != 1) abort();
    if (FAIL_FRAME) {
        static const char event[] = "frame:allocation-failed\n";
        if (write(2, event, sizeof event - 1) != (ssize_t)(sizeof event - 1)) abort();
        return NULL;
    }
    owned_frame = __real_calloc(count, size);
    if (!owned_frame) abort();
    static const char event[] = "frame:allocated\n";
    if (write(2, event, sizeof event - 1) != (ssize_t)(sizeof event - 1)) abort();
    return owned_frame;
}

void __wrap_free(void *ptr) {
    if (ptr) {
        if (ptr != owned_frame || ++releases != 1) abort();
        owned_frame = NULL;
        static const char event[] = "frame:freed\n";
        if (write(2, event, sizeof event - 1) != (ssize_t)(sizeof event - 1)) abort();
    }
    __real_free(ptr);
}

__attribute__((destructor)) static void verify_frame(void) {
    if (attempts != 1 || owned_frame || releases != (unsigned)!FAIL_FRAME) abort();
}
