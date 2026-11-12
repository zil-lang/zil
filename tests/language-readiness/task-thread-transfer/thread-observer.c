#include <assert.h>
#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
static void *frame;
static unsigned allocations, frees, starts, joins, attempts, body_calls, body_drops;
static pthread_t parent;
void *__real_calloc(size_t,size_t);
void __real_free(void*);
int __real_pthread_create(pthread_t*,const pthread_attr_t*,void*(*)(void*),void*);
int __real_pthread_join(pthread_t,void**);
static void check(void) {
    assert(allocations==1 && frees==1 && attempts==1 && body_drops==1);
    if(getenv("ZIL_TASK_THREAD_FAIL")) assert(starts==0 && joins==0 && body_calls==0);
    else assert(starts==1 && joins==1 && body_calls==1);
    putchar('K');
}
void *__wrap_calloc(size_t n,size_t s) {
    void *p=__real_calloc(n,s);
    assert(!frame && p);frame=p;allocations++;assert(atexit(check)==0);return p;
}
void __wrap_free(void *p) {if(p==frame) {assert(frees++==0);}__real_free(p);}
static void *(*entry)(void*);
static void *run(void *p) {assert(!pthread_equal(parent,pthread_self()));starts++;return entry(p);}
int __wrap_pthread_create(pthread_t *t,const pthread_attr_t *a,void*(*f)(void*),void *p) {
    parent=pthread_self();assert(*(void**)p==frame);assert(attempts++==0);
    if(getenv("ZIL_TASK_THREAD_FAIL")) return EAGAIN;
    entry=f;return __real_pthread_create(t,a,run,p);
}
int __wrap_pthread_join(pthread_t t,void **p) {
    int code=__real_pthread_join(t,p);assert(code==0);joins++;
    return getenv("ZIL_TASK_THREAD_JOIN_FAIL")?EINVAL:code;
}
void thread_body_called(void) {assert(pthread_equal(parent,pthread_self()));assert(body_calls++==0);}
void thread_body_dropped(int code) {assert(code==9);assert(pthread_equal(parent,pthread_self()));assert(body_drops++==0);}
