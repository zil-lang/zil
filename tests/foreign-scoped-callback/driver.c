/* Controllable same-thread Direct event loop. No ZIL implementation or oracle. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <inttypes.h>
#include <limits.h>
typedef int64_t (*Callback)(void *, int32_t);
struct Slot { Callback callback; void *userdata; };
static struct Slot slots[4];
static int depth, mode, bodies;
static const uint64_t rejected = UINT64_C(1) << 32;
static void check(int condition, const char *reason) {
    if (!condition) { fprintf(stderr, "C driver invariant: %s\n", reason); _Exit(99); }
}
static uint64_t call(int index, char phase, int32_t value) {
    check(index >= 0 && index < depth, "callback root is live");
    check(slots[index].callback && slots[index].userdata, "callback/context are bound");
    printf("%c%d:%" PRId32 ">", phase, index, value);
    uint64_t result = (uint64_t)slots[index].callback(slots[index].userdata, value);
    printf("=%016" PRIx64 ";", result);
    return result;
}
void driver_configure(int selected) {
    check(depth == 0, "configure outside scopes");
    mode = selected; bodies = 0; setbuf(stdout, NULL);
}
void *scope_register(Callback callback, void *userdata) {
    check(callback && userdata && depth < 4, "register concrete context");
    int index = depth++;
    check(!slots[index].callback && !slots[index].userdata, "no stale C root");
    slots[index] = (struct Slot){callback, userdata};
    printf("R%d;", index);
    if (mode != 2) {
        int32_t event = mode == 6 ? INT32_MIN : mode == 8 ? 0 : 1;
        uint64_t result = call(index, 'S', event);
        check((result >> 32) == 0, "registration callback succeeds");
    }
    if (mode == 2 || mode == 3) {
        slots[index] = (struct Slot){0}; --depth;
        printf("N%d;", index);
        return NULL; /* No retained/queued entry or userdata remains. */
    }
    return &slots[index];
}
void scope_close(void *token) {
    check(depth > 0 && token == &slots[depth-1], "close actual registration");
    int index = depth-1;
    printf("C%d;", index);
    check(call(index, 'X', 11) == rejected, "Closing callback is rejected");
    slots[index] = (struct Slot){0}; --depth;
    printf("Z%d;", index); /* Direct entries ended; no future entry remains. */
}
static int32_t unpack(uint64_t result) {
    check((result >> 32) == 0, "body callback succeeds");
    uint32_t bits = (uint32_t)result;
    int32_t value;
    memcpy(&value, &bits, sizeof(value));
    return value;
}
int32_t driver_pump(int32_t value) { return unpack(call(depth-1, 'P', value)); }
int32_t driver_outer(int32_t value) { check(depth == 2, "nested outer root"); return unpack(call(0, 'O', value)); }
int32_t driver_reenter(void) { check(call(depth-1, 'E', 7) == rejected, "active callback is rejected"); return 1; }
int driver_roots(void) {
    for (int i=depth; i<4; ++i) check(!slots[i].callback && !slots[i].userdata, "close cleared C roots");
    return depth;
}
int driver_body(void) { printf("B%d;", ++bodies); return bodies; }
void driver_hit(int label) { printf("F%c;", label); }
void driver_drop(int label) { printf("D%c;", label); }
void driver_factory(int label) { printf("%c;", label); }
int driver_zero(void) { return 0; }
