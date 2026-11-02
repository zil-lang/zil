/* Independent same-thread lifecycle and remaining-environment observer. */
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <stddef.h>
typedef int64_t (*Callback)(void *, int32_t);
size_t composition_live(void);
static Callback callback;
static void *userdata;
static int token, drops, bodies, registered;
static void check(int condition) { if (!condition) _Exit(99); }
__attribute__((constructor)) static void configure_output(void) { setbuf(stdout, NULL); }
void *reg(Callback entry, void *data) {
    check(entry && data && !callback && !userdata && composition_live() == 1);
    int mode = getchar();
    check(mode == '0' || mode == '1');
    registered = 1; callback = entry; userdata = data;
    printf("R;S1>");
    uint64_t result = (uint64_t)callback(userdata, 1);
    printf("=%016" PRIx64 ";", result);
    check(result == 2);
    if (mode == '1') {
        callback = NULL; userdata = NULL;
        printf("N;");
        return NULL;
    }
    return &token;
}
void close(void *actual) {
    check(actual == &token && callback && userdata && composition_live() == 0);
    check(drops == 0 || drops == 3); /* Frozen originals have no Drop observer. */
    printf("C;X11>");
    uint64_t result = (uint64_t)callback(userdata, 11);
    printf("=%016" PRIx64 ";", result);
    check(result == (UINT64_C(1) << 32));
    callback = NULL; userdata = NULL;
    printf("Z;");
}
void composition_body(int32_t value) {
    check(callback && userdata && value == 4 && ++bodies == 1 && composition_live() == 1);
    printf("B%d;", value);
}
void composition_drop(int32_t value) {
    check(value == 3 || value == 7);
    int bit = value == 3 ? 1 : 2;
    check(!(drops & bit)); drops |= bit;
    printf("D%d;", value);
}
void composition_after(int32_t value) {
    check(value == 5 && !callback && !userdata && composition_live() == 0);
    check(drops == (registered ? 3 : 1));
    printf("A%d;", value);
}
