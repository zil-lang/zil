/* Reuse the actual allocator shim and expose its live count to lifecycle C. */
#include "../composed-store-closures/allocator_probe.c"
size_t preparation_live(void) { return live; }
