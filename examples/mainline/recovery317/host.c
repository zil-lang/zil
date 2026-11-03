/* Explicit foreign event loop and lifecycle observer. No generated ZIL logic. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef int64_t (*Callback)(void *, int32_t);
static Callback saved;
static void *context;
static int token, mode, registered, closed, panel_drops, body_drops, handler_drops;
static void check(int condition) { if (!condition) _Exit(99); }
int32_t component_mode(void) {
    mode = getchar() - '0';
    check(mode >= 0 && mode <= 2);
    return mode;
}
void *component_register(Callback callback, void *data) {
    check(callback && data && !saved && !registered);
    saved = callback;
    context = data;
    registered = 1;
    putchar('R');
    check(saved(context, 1) == 2);
    if (mode == 2) { saved = NULL; context = NULL; putchar('N'); return NULL; }
    return &token;
}
int32_t component_pump(int32_t value) {
    check(saved && context && !closed && value == 5);
    putchar('V');
    int64_t result = saved(context, value);
    check(result == 6);
    return (int32_t)result;
}
void component_close(void *actual) {
    check(actual == &token && saved && context && panel_drops == 1 && body_drops == 1);
    check((uint64_t)saved(context, 99) == (UINT64_C(1) << 32));
    saved = NULL;
    context = NULL;
    closed = 1;
    putchar('C');
}
void component_drop(int32_t code) {
    if (code == 'P') { check(!closed && ++panel_drops == 1); }
    else if (code == 'B') { check(!closed && ++body_drops == 1); }
    else if (code == 'H') {
        check(!saved && !context && ++handler_drops == 1 && body_drops == 1);
        check(mode == 2 ? !closed && panel_drops == 0 : closed && panel_drops == 1);
    } else { check(0); }
    putchar(code);
}
void component_after(void) {
    check(!saved && !context && body_drops == 1 && handler_drops == 0);
    check(mode == 2 ? !closed && panel_drops == 0 : closed && panel_drops == 1);
    putchar('A');
}
