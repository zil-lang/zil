/* Count the live Task frame as well as the Map's actual Store allocation.
 * Failure indices still refer to Map realloc calls; Task failure is separate. */
#include "../composed-store-closures/allocator_probe.c"
void *__real_calloc(size_t, size_t);
void *__wrap_calloc(size_t count, size_t size) {
    if (getenv("ZIL_TEST_FAIL_TASK_ALLOC")) { ++failures; return NULL; }
    if (count && size > SIZE_MAX / count) _Exit(116);
    void *pointer = __real_calloc(count, size);
    if (!pointer) { ++failures; return NULL; }
    size_t index = 0;
    while (index < 128 && entries[index].p) ++index;
    if (index == 128) _Exit(113);
    entries[index] = (struct Entry){pointer, count * size};
    ++allocated; ++live; bytes_live += count * size;
    if (live > peak_live) peak_live = live;
    if (bytes_live > peak_bytes) peak_bytes = bytes_live;
    return pointer;
}
