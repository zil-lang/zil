/* Real runtime realloc/free wrappers, with a read-only lifecycle observation. */
#include "../173/allocator_probe.c"
size_t composition_live(void) { return live; }
