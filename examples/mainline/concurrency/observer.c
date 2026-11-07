/* Test observer only: generated worker code and pthread adapter are production. */
#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static pthread_t parent_thread, child_thread;
static pthread_mutex_t gate = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t wake = PTHREAD_COND_INITIALIZER;
static void *(*production_start)(void *);
static void *production_context;
static int mode, payloads, prepared, bodies, drops, creates, joins, entered, finished;
static void check(int condition) { if (!condition) _Exit(99); }
static void parent(void) { check(pthread_equal(parent_thread, pthread_self())); }
int __real_pthread_create(pthread_t *, const pthread_attr_t *, void *(*)(void *), void *);
int __real_pthread_join(pthread_t, void **);

static void *observe_start(void *unused) {
    check(unused == NULL && !pthread_equal(parent_thread, pthread_self()));
    check(pthread_mutex_lock(&gate) == 0);
    ++entered;
    while (!bodies) check(pthread_cond_wait(&wake, &gate) == 0);
    check(pthread_mutex_unlock(&gate) == 0);
    void *result = production_start(production_context);
    check(pthread_mutex_lock(&gate) == 0);
    ++finished;
    check(pthread_mutex_unlock(&gate) == 0);
    return result;
}
int __wrap_pthread_create(pthread_t *thread, const pthread_attr_t *attr,
                          void *(*start)(void *), void *context) {
    parent();
    check(++creates == 1 && payloads == 1 && prepared == 1 && !bodies && !drops);
    check(thread && !attr && start && context);
    putchar('C');
    if (mode == 2) return EAGAIN;
    production_start = start;
    production_context = context;
    int result = __real_pthread_create(thread, attr, observe_start, NULL);
    if (!result) child_thread = *thread;
    return result;
}
int __wrap_pthread_join(pthread_t thread, void **result) {
    parent();
    check(mode != 2 && ++joins == 1 && bodies == 1 && drops == 1);
    check(pthread_equal(thread, child_thread));
    if (mode == 3) { putchar('X'); return EINVAL; }
    int status = __real_pthread_join(thread, result);
    check(status == 0 && entered == 1 && finished == 1);
    putchar('J');
    return status;
}
int32_t observer_mode(void) {
    setbuf(stdout, NULL);
    parent_thread = pthread_self();
    mode = getchar() - '0';
    check(mode >= 0 && mode <= 3);
    return mode;
}
void observer_payload(void) { parent();check(++payloads == 1 && !prepared);putchar('P'); }
void observer_prepared(void) { parent();check(++prepared == 1 && payloads == 1);putchar('E'); }
void observer_body(void) {
    parent();
    check(pthread_mutex_lock(&gate) == 0);
    check(++bodies == 1 && mode != 2);
    putchar('B');
    check(pthread_cond_broadcast(&wake) == 0);
    check(pthread_mutex_unlock(&gate) == 0);
}
void observer_drop(int32_t code) {
    parent();check(code == 'D' && ++drops == 1 && creates == 1);putchar(code);
}
void observer_after(int32_t created, int32_t child, int32_t body) {
    parent();check(drops == 1 && creates == 1);
    if (mode == 2) check(!created && !child && !body && !joins && !bodies && !entered);
    else check(created == 1 && child == 21 && body == (mode == 1 ? -7 : 17) && joins == 1 && finished == 1);
    putchar('A');
}
