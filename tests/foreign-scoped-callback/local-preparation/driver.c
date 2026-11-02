/* Direct C lifecycle observer, independent of the ZIL implementation/oracle. */
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <stddef.h>
typedef int64_t (*Callback)(void *, int32_t);
size_t preparation_live(void);
static Callback callback;
static void *userdata;
static int token, drops, bodies;
static void check(int condition) { if (!condition) _Exit(99); }
__attribute__((constructor)) static void configure_output(void) { setbuf(stdout, NULL); }
void *reg(Callback entry, void *data) {
    check(entry && data && !callback && !userdata && preparation_live() == 1);
    int mode = getchar();
    check(mode == '0' || mode == '1');
    callback = entry; userdata = data;
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
    check(actual == &token && callback && userdata && preparation_live() == 0);
    printf("C;X11>");
    uint64_t result = (uint64_t)callback(userdata, 11);
    printf("=%016" PRIx64 ";", result);
    check(result == (UINT64_C(1) << 32));
    callback = NULL; userdata = NULL;
    printf("Z;");
}
void preparation_body(int32_t value) {
    check(callback && userdata && value == 4 && ++bodies == 1 && preparation_live() == 1);
    printf("B%d;", value);
}
void preparation_drop(int32_t value) {
    check(value == 3 && ++drops == 1);
    printf("D%d;", value);
}
void preparation_after(int32_t value) {
    check(value == 5 && !callback && !userdata && drops == 1 && preparation_live() == 0);
    printf("A%d;", value);
}
