/* Observation only: child entry, load and pthread adapter are generated ZIL. */
#include <errno.h>
#include <pthread.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
struct context { const int32_t *input; int32_t result; };
_Static_assert(sizeof(struct context)==16 && _Alignof(struct context)==8 && offsetof(struct context,result)==8,"shared context ABI");
static pthread_t parent_thread, child_thread;
static pthread_mutex_t gate=PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t wake=PTHREAD_COND_INITIALIZER;
static void *(*production_start)(void *);
static struct context *production_context;
static const int32_t *original;
static int mode,bodies,drops,creates,joins,entered,finished;
static void check(int condition) { if (!condition) _Exit(99); }
static void parent(void) { check(pthread_equal(parent_thread,pthread_self())); }
int __real_pthread_create(pthread_t *,const pthread_attr_t *,void *(*)(void *),void *);
int __real_pthread_join(pthread_t,void **);
static void *observe_start(void *unused) {
    check(!unused && !pthread_equal(parent_thread,pthread_self()));
    check(pthread_mutex_lock(&gate)==0);
    ++entered;
    while (!drops) check(pthread_cond_wait(&wake,&gate)==0);
    check(bodies==1 && original==production_context->input && *original==9);
    check(pthread_mutex_unlock(&gate)==0);
    void *result=production_start(production_context);
    check(pthread_mutex_lock(&gate)==0);
    check(production_context->result==11);
    ++finished;
    check(pthread_mutex_unlock(&gate)==0);
    return result;
}
int __wrap_pthread_create(pthread_t *thread,const pthread_attr_t *attr,void *(*start)(void *),void *context) {
    parent();check(++creates==1 && !bodies && !drops && thread && !attr && start && context);
    production_start=start;production_context=context;original=production_context->input;
    check(original && (void *)original!=context && *original==9 && !production_context->result);
    putchar('C');
    if (mode==2) return EAGAIN;
    int status=__real_pthread_create(thread,attr,observe_start,NULL);
    if (!status) child_thread=*thread;
    return status;
}
int __wrap_pthread_join(pthread_t thread,void **result) {
    parent();check(mode!=2 && ++joins==1 && bodies==1 && drops==1 && pthread_equal(thread,child_thread));
    if (mode==3) { putchar('X');return EINVAL; }
    int status=__real_pthread_join(thread,result);
    check(!status && entered==1 && finished==1);putchar('J');return status;
}
int32_t shared_mode(void) {
    setbuf(stdout,NULL);parent_thread=pthread_self();mode=getchar()-'0';check(mode>=0 && mode<=3);return mode;
}
void shared_body(const int32_t *input) {
    parent();check(pthread_mutex_lock(&gate)==0);
    check(mode!=2 && ++bodies==1 && input==original && *input==9);putchar('B');
    check(pthread_mutex_unlock(&gate)==0);
}
void shared_drop(int32_t code) {
    parent();check(pthread_mutex_lock(&gate)==0);
    check(code==68 && creates==1 && ++drops==1);putchar('D');
    check(pthread_cond_broadcast(&wake)==0);check(pthread_mutex_unlock(&gate)==0);
}
void shared_after(int32_t created,int32_t child,int32_t body) {
    parent();check(creates==1 && drops==1);
    if (mode==2) check(!created && !child && !body && !bodies && !joins && !entered);
    else check(created==1 && child==11 && body==(mode==1?-7:9) && joins==1 && finished==1);
    putchar('A');
}
