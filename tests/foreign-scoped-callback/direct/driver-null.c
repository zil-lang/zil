#include <stdint.h>
#include <stdlib.h>

typedef int64_t (*Callback)(void *, int32_t);
void *reg(Callback callback, void *userdata) {
    if (!callback || !userdata) _Exit(91);
    if ((uint64_t)callback(userdata, -11) != UINT64_C(0x00000000fffffff6)) _Exit(92);
    return 0; /* The synchronous entry has ended; no code or context is retained. */
}
void close(void *token) { (void)token; _Exit(93); }
int32_t body_called(void) { _Exit(94); }
