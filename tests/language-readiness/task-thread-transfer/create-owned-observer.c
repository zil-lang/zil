#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
static unsigned attempts,allocations,frees,first,second;
static void *frame;
void *__real_calloc(size_t,size_t);
void __real_free(void*);
static void check(void){assert(attempts==1 && first==1 && second==1);if(getenv("ZIL_TASK_ALLOC_FAIL"))assert(!allocations && !frees);else assert(allocations==1 && frees==1);puts("both original owners cleaned once");}
void *__wrap_calloc(size_t n,size_t s){assert(attempts++==0);assert(atexit(check)==0);if(getenv("ZIL_TASK_ALLOC_FAIL"))return NULL;frame=__real_calloc(n,s);assert(frame);allocations++;return frame;}
void __wrap_free(void *p){assert(p==frame && first==1 && second==1 && frees++==0);__real_free(p);}
void owned_input_drop(int code){if(code==37)assert(first++==0);else {assert(code==43 && second++==0);}}
