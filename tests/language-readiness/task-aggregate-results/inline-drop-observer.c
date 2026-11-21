#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

struct Mark { int32_t code; };
struct Pair { struct Mark before; void *child; struct Mark after; };
struct Outcome { bool error; struct Mark rejected; struct Pair value; };
struct Capture { struct Pair value; bool shell, before, child, after; uint32_t cursor; };
struct Node {
    struct Node *prev, *next;
    void *drop, *value;
    bool (*poll)(void *, uint32_t *);
    void (*request)(void *, uint32_t *);
    uint32_t cursor;
    bool inline_call;
};

extern void link_owner(struct Node *, struct Node *) __asm__("zil.task.owner.link");
extern bool drain(struct Node *) __asm__("zil.task.owner.drain");
extern void request_all(struct Node *) __asm__("zil.task.owner.request");
extern bool poll_inline(void *, uint32_t *) __asm__("test.inline.poll");
extern void request_inline(void *, uint32_t *) __asm__("test.inline.request");
extern bool poll_capture(struct Capture *) __asm__("zil.task.group.0.poll");
extern void request_capture(struct Capture *) __asm__("zil.task.group.0.request");

static void *live_child;
static unsigned polls, requests, finishes, marks;
static bool cancel_requested;

static void check(bool value) { if (!value) _Exit(77); }

void mark_drop(void *value) __asm__("zil.fn.0");
void mark_drop(void *value) {
    char code = (char)((struct Mark *)value)->code;
    check(code == 'A' || code == 'B' || code == 'X');
    ++marks;
    putchar(code);
}

void child_request(void *value) __asm__("zil.task.request_cancel");
void child_request(void *value) {
    check(*(void **)value == live_child && live_child != NULL);
    ++requests;
    cancel_requested = true;
}

bool child_poll(void *value) __asm__("zil.task.poll_close");
bool child_poll(void *value) {
    check(*(void **)value == live_child && live_child != NULL);
    check(cancel_requested);
    return ++polls == 3;
}

void child_finish(void *frame) __asm__("zil.task.finish_destroy");
void child_finish(void *frame) {
    check(frame == live_child && frame != NULL && polls == 3);
    ++finishes;
    putchar('T');
    free(live_child);
    live_child = NULL;
}

static void enter(struct Node *anchor, struct Node *owner, struct Outcome *value) {
    owner->value = value;
    link_owner(owner, anchor);
    check(owner->cursor == 0);
}

int main(int argc, char **argv) {
    check(argc == 2);
    bool cancel = argv[1][0] == 'c';
    unsigned expected_requests = cancel ? 5 : 3;
    struct Node anchor = {0};
    anchor.prev = anchor.next = &anchor;
    struct Node owner = {.poll = poll_inline, .request = request_inline, .cursor = 89, .inline_call = true};
    live_child = malloc(1);
    check(live_child != NULL);
    struct Outcome selected = {false, {'X'}, {{'A'}, live_child, {'B'}}};
    enter(&anchor, &owner, &selected);
    if (cancel) request_all(&anchor);
    check(requests == (unsigned)cancel && marks == 0);
    check(!drain(&anchor));
    check(owner.next == &anchor && owner.cursor == 2 && marks == 1 && polls == 1);
    if (cancel) request_all(&anchor);
    check(requests == (cancel ? 3u : 1u) && marks == 1);
    check(!drain(&anchor));
    check(owner.next == &anchor && owner.cursor == 2 && marks == 1 && polls == 2);
    check(drain(&anchor));
    check(owner.next == NULL && selected.value.child == NULL && marks == 2 && finishes == 1);
    request_all(&anchor);
    check(drain(&anchor) && requests == expected_requests);

    /* Reuse the real node for a new occupant with a different active tag. */
    struct Outcome alternate = {true, {'X'}, {{'A'}, (void *)(uintptr_t)1, {'B'}}};
    enter(&anchor, &owner, &alternate);
    request_all(&anchor);
    check(drain(&anchor) && marks == 3 && requests == expected_requests && finishes == 1);
    check(owner.next == NULL && owner.prev == NULL);

    live_child = malloc(1);
    check(live_child != NULL);
    polls = requests = 0;
    cancel_requested = false;
    struct Capture capture = {{{-99}, live_child, {'B'}}, true, false, true, true, 0};
    if (cancel) request_capture(&capture);
    check(!poll_capture(&capture));
    check(capture.shell && capture.child && capture.after && marks == 3 && polls == 1);
    unsigned stopped = capture.cursor;
    if (cancel) request_capture(&capture);
    check(!poll_capture(&capture));
    check(capture.cursor == stopped && capture.shell && capture.child && marks == 3 && polls == 2);
    check(poll_capture(&capture));
    check(!capture.shell && !capture.before && !capture.child && !capture.after && capture.value.child == NULL);
    request_capture(&capture);
    check(poll_capture(&capture) && marks == 4 && requests == expected_requests && finishes == 2);
    return 0;
}
