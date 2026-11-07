/* Real cross-thread F and pre-trampoline lifetime consumer. The relaxed handoff
 * orders attempts only: F's non-atomic count relies on the generated gate's
 * release/acquire, not a provider mutex or join between the two F calls. */
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef int64_t (*callback_t)(void *, int32_t);
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t changed = PTHREAD_COND_INITIALIZER;
static pthread_t holder, racer;
static callback_t callback;
static void *context, *frame, *buffer;
static atomic_int phase, race_go;
static int raced, race_joined;
static int acquired, released, joined, closing, installed;
static int requests, polls, rejected, frame_allocations, frame_frees;
static int buffer_allocations, buffer_frees, cleanup_reentries, provider_reentries, request_reentries;
static const int64_t denied = INT64_C(1) << 32;
static void check(int truth) { if (!truth) abort(); }
static void take_lock(void) { check(pthread_mutex_lock(&lock) == 0); }
static void give_lock(void) { check(pthread_mutex_unlock(&lock) == 0); }
static void changed_signal(void) { check(pthread_cond_broadcast(&changed) == 0); }
static void changed_wait(void) { check(pthread_cond_wait(&changed, &lock) == 0); }
void *__real_calloc(size_t, size_t);
void *__real_realloc(void *, size_t);
void __real_free(void *);
void *__wrap_realloc(void *p, size_t n) {
    check(!buffer && p == NULL && n >= 4);
    buffer = __real_realloc(p, n);
    check(buffer != NULL);
    buffer_allocations++;
    return buffer;
}
void *__wrap_calloc(size_t count, size_t size) {
    if (getenv("ZIL_THREAD_ALLOC_FAIL")) return NULL;
    frame = __real_calloc(count, size);
    check(frame != NULL && frame_allocations == 0);
    frame_allocations++;
    return frame;
}
void __wrap_free(void *p) {
    if (p && p == buffer) {
        check(buffer_frees == 0 && (!installed || joined) && !context && !callback);
        if (frame && closing) {
            /* Private-runtime reentry control: this prefix is the actual thunk
             * prefix, not a new Source capability or an external owner API. */
            bool (*poll)(void *) = ((bool (**)(void *))frame)[1];
            check(!poll(frame));
            check(requests == 1 && polls == 2);
            cleanup_reentries++;
        }
        buffer = NULL;
        buffer_frees++;
        if (getenv("ZIL_THREAD_TRACE")) { puts("buffer-free"); fflush(stdout); }
    } else if (p && p == frame) {
        check(buffer_frees == 1 && !context && !callback && (!installed || joined));
        frame = NULL;
        frame_frees++;
        if (getenv("ZIL_THREAD_TRACE")) { puts("frame-free"); fflush(stdout); }
    }
    __real_free(p);
}
static void *first_call(void *unused) {
    (void)unused;
    check(callback(context, 2) == 40);
    atomic_store_explicit(&phase, 1, memory_order_relaxed);
    return NULL;
}
static void *second_call(void *unused) {
    (void)unused;
    while (!atomic_load_explicit(&phase, memory_order_relaxed)) sched_yield();
    check(callback(context, 3) == 42);
    atomic_store_explicit(&phase, 2, memory_order_relaxed);
    return NULL;
}
static void *resumed_call(void *unused) {
    (void)unused;
    /* The ZIL install helper has returned before thread_race is called. */
    check(callback(context, 4) == 44);
    return NULL;
}
static void *race_close(void *unused) {
    (void)unused;
    while (!atomic_load_explicit(&race_go, memory_order_relaxed)) sched_yield();
    int64_t answer = callback(context, 5);
    /* One finite competing entry, with both CAS orders allowed. This observes
     * a real consumer race; it does not claim a particular gate interleaving. */
    check(answer == denied || answer == 46);
    raced = 1;
    return NULL;
}
int32_t thread_race(void) {
    check(installed && !closing && !raced && !race_joined);
    pthread_t resumed;
    check(pthread_create(&resumed, NULL, resumed_call, NULL) == 0);
    check(pthread_join(resumed, NULL) == 0);
    check(pthread_create(&racer, NULL, race_close, NULL) == 0);
    atomic_store_explicit(&race_go, 1, memory_order_relaxed);
    return 0;
}
static void *hold_raw(void *unused) {
    (void)unused;
    take_lock();
    callback_t raw_callback = callback;
    void *raw_context = context;
    acquired = 1;
    changed_signal();
    while (!released) changed_wait();
    give_lock();
    if (!getenv("ZIL_THREAD_INSTALL_FAIL")) {
        check(raw_callback(raw_context, 101) == denied);
        /* This write is acquired by join before the provider returns drain1. */
        rejected++;
    }
    return NULL;
}
static void release_holder(void) {
    take_lock();
    check(acquired && !released && !joined);
    released = 1;
    changed_signal();
    give_lock();
    check(pthread_join(holder, NULL) == 0);
    joined = 1;
}
void *thread_register(callback_t cb, void *ctx) {
    check(!installed && frame == ctx && buffer && frame_allocations == 1);
    callback = cb;
    context = ctx;
    pthread_t a, b;
    check(pthread_create(&a, NULL, first_call, NULL) == 0);
    check(pthread_create(&b, NULL, second_call, NULL) == 0);
    check(pthread_join(a, NULL) == 0 && pthread_join(b, NULL) == 0);
    check(atomic_load_explicit(&phase, memory_order_relaxed) == 2);
    check(pthread_create(&holder, NULL, hold_raw, NULL) == 0);
    installed = 1;
    take_lock();
    while (!acquired) changed_wait();
    give_lock();
    if (getenv("ZIL_THREAD_INSTALL_FAIL")) {
        release_holder();
        callback = NULL;
        context = NULL;
        return NULL;
    }
    if (getenv("ZIL_THREAD_TRACE")) { puts("installed frame=1 buffer=1"); fflush(stdout); }
    return &holder;
}
void thread_request(void *token) {
    check(token == &holder && !closing && acquired);
    closing = 1;
    requests++;
    check(pthread_join(racer, NULL) == 0 && raced);
    race_joined = 1;
    bool (*poll)(void *) = ((bool (**)(void *))frame)[1];
    check(!poll(frame));
    request_reentries++;
    check(callback(context, 100) == denied);
    rejected++;
}
int32_t thread_drain(void *token) {
    check(token == &holder && closing && !buffer_frees && !frame_frees);
    polls++;
    bool (*poll)(void *) = ((bool (**)(void *))frame)[1];
    check(!poll(frame));
    provider_reentries++;
    if (getenv("ZIL_THREAD_INVALID_DRAIN")) {
        puts("invalid drain; frame=1 buffer=1"); fflush(stdout);
        return 2;
    }
    if (!joined) return 0;
    check(rejected == 2);
    callback = NULL;
    context = NULL;
    return 1;
}
int32_t thread_release(void) {
    check(closing && polls == 1 && !frame_frees && !buffer_frees);
    release_holder();
    return 0;
}
int32_t thread_complete(int32_t failed) {
    check(buffer_allocations == 1 && buffer_frees == 1 && frame_allocations == frame_frees);
    if (failed) {
        check(!requests && !polls && !cleanup_reentries && !provider_reentries && !request_reentries && (!installed || joined));
        puts("threaded construction failure drained; owned buffer freed once");
    } else {
        check(installed && joined && raced && race_joined && requests == 1 && polls == 2 && rejected == 2 && cleanup_reentries == 1 && provider_reentries == 2 && request_reentries == 1);
        puts("thread F writes acquired; raw holder drained; cleanup reentry pending; once free");
    }
    return 0;
}
