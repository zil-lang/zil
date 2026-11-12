#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
typedef int64_t (*callback_t)(void*,int32_t);
static void *registration,*task;
static callback_t callback;
static unsigned attempts,reg_alloc,task_alloc,reg_free,task_free,requests,polls,denied;
static const int64_t rejected=INT64_C(1)<<32;
void *__real_calloc(size_t,size_t);
void __real_free(void*);
static void check(void){assert(attempts==2 && reg_alloc==1 && reg_free==1 && requests==1 && polls==2 && denied==2 && !callback);if(getenv("ZIL_TASK_ALLOC_FAIL"))assert(!task_alloc && !task_free);else assert(task_alloc==1 && task_free==1);puts("original active input drained after allocation outcome; once free");}
void *__wrap_calloc(size_t n,size_t s){attempts++;assert(attempts<=2);if(attempts==1){registration=__real_calloc(n,s);assert(registration);reg_alloc++;assert(atexit(check)==0);return registration;}assert(callback && !reg_free && !requests && !polls);if(getenv("ZIL_TASK_ALLOC_FAIL"))return NULL;task=__real_calloc(n,s);assert(task);task_alloc++;return task;}
void __wrap_free(void *p){if(p==registration){assert(!callback && denied==2 && polls==2 && reg_free++==0);}else{assert(p==task && reg_free==1 && task_free++==0);}__real_free(p);}
void *reg(callback_t cb,void *ctx){assert(ctx==registration && !callback);callback=cb;assert(cb(ctx,2)==40);return registration;}
void request(void *token){assert(token==registration && requests++==0 && !reg_free);assert(callback(registration,100)==rejected);denied++;}
int32_t drain(void *token){assert(token==registration && requests==1 && !reg_free);polls++;assert(polls<=2);if(polls==1)return 0;assert(callback(registration,101)==rejected);denied++;callback=NULL;return 1;}
