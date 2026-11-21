#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
static unsigned attempts,allocations,frees,drops,second_drop,entries,ready;
static void *frame;
void *__real_calloc(size_t,size_t);
void __real_free(void*);
static void check(void){
 assert(attempts==1 && drops==1 && second_drop==(getenv("ZIL_MIXED_TWO")?1u:0u));
 if(getenv("ZIL_TASK_ALLOC_FAIL"))assert(!allocations && !frees && !entries && !ready);
 else {assert(allocations==1 && frees==1 && entries==1);assert(ready==(getenv("ZIL_EXPECT_CANCEL")?0u:1u));}
 puts("mixed original owner and borrow survived exactly once");
}
void *__wrap_calloc(size_t n,size_t s){assert(attempts++==0);assert(atexit(check)==0);if(getenv("ZIL_TASK_ALLOC_FAIL"))return NULL;frame=__real_calloc(n,s);assert(frame);allocations++;return frame;}
void __wrap_free(void *p){assert(p==frame && drops==1 && frees++==0);__real_free(p);}
void mixed_input_drop(int code){if(code==5)assert(drops++==0);else{assert(code==7 && getenv("ZIL_MIXED_TWO") && second_drop++==0);}}
void mixed_worker_entry(void){assert(entries++==0 && !ready && !drops && !second_drop);}
void mixed_worker_ready(void){assert(entries==1 && ready++==0 && !drops && !second_drop);}
