#include <stdint.h>
#include <stdlib.h>

typedef int64_t (*Callback)(void *, int32_t);
static Callback saved;
static void *context;
static int token;

void *reg(Callback callback, void *userdata) {
    if (!callback || !userdata || saved) _Exit(91);
    saved = callback;
    context = userdata;
    if ((uint64_t)saved(context, 1) != UINT64_C(2)) _Exit(92);
    return &token;
}

void close(void *handle) {
    if (handle != &token || !saved || !context) _Exit(93);
    if ((uint64_t)saved(context, 11) != (UINT64_C(1) << 32)) _Exit(94);
    saved = 0;
    context = 0;
}
