/* Test observer: both starts invoke the production adapter on real OS threads. */
#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
static pthread_t parent_thread;
static pthread_mutex_t gate=PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t wake=PTHREAD_COND_INITIALIZER;
static int released;
static struct { pthread_t thread; void *(*start)(void *); void *context; int entered, finished; } slots[2];
static int mode, creates, joins, bodies[2], drops[2];
static void check(int condition) { if (!condition) _Exit(99); }
static void parent(void) { check(pthread_equal(parent_thread,pthread_self())); }
int __real_pthread_create(pthread_t *,const pthread_attr_t *,void *(*)(void *),void *);
int __real_pthread_join(pthread_t,void **);
static void *start(void *argument) {
    intptr_t index=(intptr_t)argument;
    check(index>=0 && index<2 && !pthread_equal(parent_thread,pthread_self()));
    slots[index].entered=1;
    if(index==0) {
        check(pthread_mutex_lock(&gate)==0);
        while(!released) check(pthread_cond_wait(&wake,&gate)==0);
        check(pthread_mutex_unlock(&gate)==0);
    }
    void *result=slots[index].start(slots[index].context);
    slots[index].finished=1;
    return result;
}
int __wrap_pthread_create(pthread_t *thread,const pthread_attr_t *attr,void *(*entry)(void *),void *context) {
    parent();int index=creates++;check(index<2 && !attr && entry && context);putchar('C');
    if ((mode==2 && index==0)||(mode==1 && index==1)) return EAGAIN;
    if(index==1) check(context!=slots[0].context);
    slots[index].start=entry;slots[index].context=context;
    int status=__real_pthread_create(thread,attr,start,(void *)(intptr_t)index);
    check(status==0);slots[index].thread=*thread;return status;
}
int __wrap_pthread_join(pthread_t thread,void **result) {
    parent();int index=(mode==1)?0:1-joins;check(index>=0 && index<2);++joins;
    check(pthread_equal(thread,slots[index].thread) && drops[index]==1 && bodies[index]==1);
    if ((mode==3 && index==1)||(mode==4 && index==0)) { putchar(index?'x':'X');return EINVAL; }
    int status=__real_pthread_join(thread,result);
    check(status==0 && slots[index].entered==1 && slots[index].finished==1);putchar(index?'j':'J');return status;
}
int32_t nested_mode(void) { setbuf(stdout,NULL);parent_thread=pthread_self();mode=getchar()-'0';check(mode>=0 && mode<=4);return mode; }
void nested_body(int32_t which) { parent();int index=which-1;check(index>=0 && index<2 && ++bodies[index]==1);putchar(index?'b':'B'); }
void nested_drop(int32_t code) { parent();int index=code=='I';check((code=='O'||code=='I') && ++drops[index]==1);putchar(code); }
void nested_inner(int32_t created,int32_t child,int32_t body) {
    parent();check(creates==2 && drops[1]==1);
    if(mode==1) check(!created && !child && !body && !joins && !bodies[1]);
    else check(created==1 && child==13 && body==17 && joins==1 && slots[1].finished==1);
    check(pthread_mutex_lock(&gate)==0);released=1;
    check(pthread_cond_broadcast(&wake)==0);check(pthread_mutex_unlock(&gate)==0);
    putchar('a');
}
void nested_after(int32_t created,int32_t child,int32_t body) {
    parent();check(drops[0]==1);
    if(mode==2) check(!created && !child && !body && creates==1 && !joins && !bodies[0] && !drops[1]);
    else check(created==1 && child==7 && body==(mode==1?-100:30) && joins==(mode==1?1:2) && slots[0].finished==1);
    putchar('A');
}
