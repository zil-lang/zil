#define _GNU_SOURCE
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
typedef int64_t (*callback_t)(void *, int32_t);
static void *records[3];
static size_t sizes[3];
static callback_t callbacks[2];
static unsigned attempts, installs, freed[3], requests[2], polls[2], denied[2], next_request;
static const int64_t rejected=INT64_C(1)<<32;
static unsigned index_of(void *p) {for(unsigned i=0;i<3;i++) if(records[i]==p)return i;abort();}
static void check(void) {
    assert(attempts==3 && installs==2 && next_request==2);
    for(unsigned i=0;i<2;i++) assert(freed[i]==1 && !callbacks[i] && requests[i]==1 && polls[i]==2 && denied[i]==2);
    if(getenv("ZIL_TASK_ALLOC_FAIL")) assert(!records[2] && !freed[2]);
    else assert(records[2] && freed[2]==1);
    puts("two original same-type active inputs retired once through the actual helper heads");
}
void *__wrap_calloc(size_t n,size_t s) {
    unsigned i=attempts++;
    assert(i<3 && n && s && n<=SIZE_MAX/s);
    if(i==0)assert(atexit(check)==0);
    if(i==2) {
        assert(installs==2 && callbacks[0] && callbacks[1] && !requests[0] && !requests[1]);
        if(getenv("ZIL_TASK_ALLOC_FAIL"))return NULL;
    }
    size_t page=(size_t)sysconf(_SC_PAGESIZE),bytes=n*s;
    sizes[i]=(bytes+page-1)/page*page;
    records[i]=mmap(NULL,sizes[i],PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    assert(records[i]!=MAP_FAILED);
    return records[i];
}
void __wrap_free(void *p) {
    unsigned i=index_of(p);assert(freed[i]++==0);
    if(i<2)assert(!callbacks[i] && polls[i]==2 && denied[i]==2);
    else assert(freed[0]==1 && freed[1]==1);
    /* Keep a protected quarantine so any marker, poll or cleanup after the
       actual helper retired this exact head is a real machine fault. */
    assert(mprotect(p,sizes[i],PROT_NONE)==0);
}
void *reg(callback_t cb,void *ctx) {
    unsigned i=index_of(ctx);assert(i<2 && installs++==i && !callbacks[i]);
    callbacks[i]=cb;assert(cb(ctx,2)==(i==0?40:46));return ctx;
}
void request(void *token) {
    unsigned i=index_of(token);assert(i<2 && next_request++==i && !freed[i] && requests[i]++==0);
    assert(callbacks[i](token,100)==rejected);denied[i]++;
}
int32_t drain(void *token) {
    unsigned i=index_of(token);assert(i<2 && requests[i]==1 && !freed[i]);
    assert(++polls[i]<=2);if(polls[i]==1)return 0;
    assert(callbacks[i](token,101)==rejected);denied[i]++;callbacks[i]=NULL;return 1;
}
