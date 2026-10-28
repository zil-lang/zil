/* Test-only extension: report even when the consumer performs no allocation.
 * The shared shim still wraps the real runtime realloc/free entry points. */
#include "../../research/142/allocator_probe.c"

__attribute__((constructor)) static void install_composed_probe(void) {
    installed = 1;
    const char *value = getenv("ZIL_TEST_FAIL_AT");
    if (value) fail_at = (size_t)strtoull(value, NULL, 10);
    if (atexit(report)) _Exit(112);
}
