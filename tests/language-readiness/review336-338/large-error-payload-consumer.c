#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* Machine consumer of actual emitted ZIL source, LargeCopy ABI v1/x64. */
struct Failure { int32_t tag; unsigned char payload[4096]; };
struct Result { bool error; struct Failure failure; unsigned char value[4096]; };
_Static_assert(offsetof(struct Result, failure) == 4, "error alignment");
_Static_assert(offsetof(struct Result, value) == 4104, "success offset");
_Static_assert(sizeof(struct Result) == 8200, "complete result size");
extern void supply(struct Result *, bool, const unsigned char *)
    __asm__("zil.fn.0.lc1");

__attribute__((constructor)) static void inspect_actual_error_payload(void) {
    unsigned char input[4096];
    struct Result output;
    memset(input, 3, sizeof input);
    memset(&output, 0xa5, sizeof output);
    supply(&output, true, input);
    input[0] = 99;
    if (!output.error || output.failure.tag != 1 ||
        output.failure.payload[0] != 3 || output.failure.payload[4095] != 3)
        _Exit(90);
    memset(&output, 0xa5, sizeof output);
    supply(&output, false, input);
    input[4095] = 99;
    if (output.error || output.value[0] != 99 || output.value[4095] != 3)
        _Exit(91);
}
