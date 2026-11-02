/* Reuse the actual allocator shim and expose its live count to lifecycle C. */
#include "../173/allocator_probe.c"
size_t preparation_live(void) { return live; }
