/* A real raw-holder counterexample, with deterministic pthread handshakes.
 * The worker holds userdata before trampoline admission. Callback execution
 * remains on the owner thread in this same-thread provider control. */
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef int64_t (*callback_t)(void *, int32_t);
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t changed = PTHREAD_COND_INITIALIZER;
static pthread_t thread;
static callback_t retained_callback, queued_callback;
static void *retained_context, *queued_context;
static int acquired, released, joined, closing, requests, polls, rejected;
static int allocations, frees, drops, installed;
static void *frame;
static const int64_t denied = INT64_C(1) << 32;
static void check(int truth) { if (!truth) abort(); }
static void take_lock(void) { check(pthread_mutex_lock(&lock) == 0); }
static void give_lock(void) { check(pthread_mutex_unlock(&lock) == 0); }
static void signal_changed(void) { check(pthread_cond_broadcast(&changed) == 0); }
static void await_change(void) { check(pthread_cond_wait(&changed, &lock) == 0); }

void *__real_calloc(size_t, size_t);
void __real_free(void *);
void *__wrap_calloc(size_t count, size_t size) {
    if (getenv("ZIL_RAW_ALLOC_FAIL")) return NULL;
    void *p = __real_calloc(count, size);
    check(p != NULL && allocations == 0);
    frame = p;
    allocations++;
    return p;
}
void __wrap_free(void *p) {
    if (p == frame && p != NULL) {
        /* Every real raw user must have returned before this observation. */
        check(!retained_callback && !retained_context && !queued_callback &&
              !queued_context && (!installed || joined) && drops == 1);
        frees++;
        frame = NULL;
    }
    __real_free(p);
}
static void *hold_raw(void *unused) {
    (void)unused;
    take_lock();
    callback_t callback = retained_callback;
    void *context = retained_context;
    check(callback != NULL && context != NULL);
    acquired = 1;
    signal_changed();
    while (!released) await_change();
    /* Transfer this still-live raw entry to the owner-side queue. Joining this
     * thread alone does NOT yet imply no-future: the queue remains a raw user. */
    queued_callback = callback;
    queued_context = context;
    give_lock();
    return NULL;
}
static void release_holder(void) {
    take_lock();
    check(acquired && !released && !joined);
    released = 1;
    signal_changed();
    give_lock();
    check(pthread_join(thread, NULL) == 0);
    joined = 1;
}
void *raw_register(callback_t callback, void *context) {
    check(!installed && allocations == 1);
    retained_callback = callback;
    retained_context = context;
    /* Ordinary FnMut mutation is observed before the raw holder starts. */
    check(callback(context, 2) == 40 && callback(context, 3) == 42);
    check(pthread_create(&thread, NULL, hold_raw, NULL) == 0);
    installed = 1;
    take_lock();
    while (!acquired) await_change();
    give_lock();
    if (getenv("ZIL_RAW_INSTALL_FAIL")) {
        /* Null installation still drains the real pre-trampoline holder and
         * then removes its transferred queue entry before returning. */
        release_holder();
        queued_callback = retained_callback = NULL;
        queued_context = retained_context = NULL;
        return NULL;
    }
    return &thread;
}
void raw_request(void *token) {
    check(token == &thread && acquired && !closing);
    closing = 1;
    requests++;
    check(retained_callback(retained_context, 100) == denied);
    rejected++;
}
int32_t raw_drain(void *token) {
    check(token == &thread && closing && drops == 0 && frees == 0);
    polls++;
    /* Both the stalled thread and the transferred raw queue are actual users.
     * No callback is active here; active==0 must still yield Pending. */
    if (!joined || queued_context != NULL) return 0;
    retained_callback = NULL;
    retained_context = NULL;
    return 1;
}
int32_t raw_release(void) {
    check(closing && polls == 1 && drops == 0 && allocations - frees == 1);
    release_holder();
    check(queued_callback && queued_context);
    check(queued_callback(queued_context, 101) == denied);
    rejected++;
    queued_callback = NULL;
    queued_context = NULL;
    return 0;
}
void raw_note(int32_t code) {
    check(code == 37 && drops == 0 && !retained_context && !queued_context &&
          !retained_callback && !queued_callback && (!installed || joined));
    drops++;
}
int32_t raw_failure_complete(void) {
    check(drops == 1 && allocations == frees && requests == 0 && polls == 0);
    puts("failed construction drained raw holders; environment dropped once");
    return 0;
}
int32_t raw_complete(void) {
    check(acquired && released && joined && requests == 1 && polls == 2 &&
          rejected == 2 && drops == 1 && allocations == 1 && frees == 1);
    puts("pre-trampoline holder kept frame live; no-future before once cleanup");
    return 0;
}
