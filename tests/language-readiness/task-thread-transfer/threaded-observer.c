#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <errno.h>
#include <poll.h>
#include <pthread.h>
#include <sched.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
typedef int64_t (*callback_t)(void*,int32_t);
static void *task_frame,*registration,*buffer;
static callback_t callback;
static pthread_t parent,callback_thread,raw_thread,driver_thread;
static int entered[2],resume_callback[2],raw_resume[2];
static atomic_int closing,polls,raw_acquired,drained;
static unsigned allocs,task_frees,reg_frees,buffer_frees,attempts,driver_started,driver_joined,body_calls,requests,rejected;
static int failed;
static const int64_t denied=INT64_C(1)<<32;
void *__real_calloc(size_t,size_t);
void *__real_realloc(void*,size_t);
void __real_free(void*);
int __real_pthread_create(pthread_t*,const pthread_attr_t*,void*(*)(void*),void*);
int __real_pthread_join(pthread_t,void**);
static atomic_int *gate(void){return (atomic_int*)((char*)registration+4*sizeof(void*));}
static void send_byte(int fd){assert(write(fd,"x",1)==1);}
static void receive_byte(int fd){struct pollfd p={fd,POLLIN,0};assert(poll(&p,1,5000)==1);char c;assert(read(fd,&c,1)==1);}
static void wait_atomic(atomic_int *p,int wanted){struct timespec start,now;assert(clock_gettime(CLOCK_MONOTONIC,&start)==0);while(atomic_load_explicit(p,memory_order_acquire)!=wanted){sched_yield();assert(clock_gettime(CLOCK_MONOTONIC,&now)==0);assert(now.tv_sec-start.tv_sec<5);}}
static void pause_callback(int signo){(void)signo;char c;if(write(entered[1],"x",1)!=1)_exit(80);if(read(resume_callback[0],&c,1)!=1)_exit(81);}
void *__wrap_calloc(size_t n,size_t s){void *p=__real_calloc(n,s);assert(p);if(allocs++==0){task_frame=p;parent=pthread_self();alarm(15);}else{assert(allocs==2);registration=p;}return p;}
void *__wrap_realloc(void *p,size_t s){assert(!p && !buffer && s>=4);buffer=__real_realloc(p,s);assert(buffer);return buffer;}
void __wrap_free(void *p){if(p==buffer && p){assert(atomic_load(&drained)==1 && buffer_frees++==0);buffer=NULL;}else if(p==registration && p){assert(buffer_frees==1 && atomic_load(&drained)==1 && reg_frees++==0);}else if(p==task_frame && p){assert(reg_frees==1 && task_frees++==0);}__real_free(p);}
static void *(*driver_entry)(void*);
static void *run_driver(void *p){assert(!pthread_equal(parent,pthread_self()));driver_started++;return driver_entry(p);}
int __wrap_pthread_create(pthread_t *t,const pthread_attr_t *a,void*(*entry)(void*),void *p){if(p && *(void**)p==task_frame){assert(attempts++==0);if(failed)return EAGAIN;driver_entry=entry;int code=__real_pthread_create(t,a,run_driver,p);driver_thread=*t;return code;}return __real_pthread_create(t,a,entry,p);}
int __wrap_pthread_join(pthread_t t,void **p){int code=__real_pthread_join(t,p);if(!failed && pthread_equal(t,driver_thread))driver_joined++;return code;}
static void *call_external(void *p){(void)p;int noise=0;for(int i=20000000;i>0;--i)noise=(noise+i)%997;assert(callback(registration,20000000)==20000000+38+noise);return NULL;}
static void *hold_raw(void *p){(void)p;callback_t cb=callback;void *ctx=registration;atomic_store_explicit(&raw_acquired,1,memory_order_release);receive_byte(raw_resume[0]);assert(cb(ctx,100)==denied);return NULL;}
void *thread_register(callback_t cb,void *ctx){assert(ctx==registration && allocs==2 && buffer);failed=getenv("ZIL_TASK_THREAD_FAIL")!=NULL;callback=cb;assert(pipe(entered)==0 && pipe(resume_callback)==0 && pipe(raw_resume)==0);struct sigaction action={0};action.sa_handler=pause_callback;assert(sigemptyset(&action.sa_mask)==0 && sigaction(SIGUSR1,&action,NULL)==0);assert(pthread_create(&callback_thread,NULL,call_external,NULL)==0);wait_atomic(gate(),1);assert(pthread_kill(callback_thread,SIGUSR1)==0);receive_byte(entered[0]);assert(atomic_load(gate())==1);assert(pthread_create(&raw_thread,NULL,hold_raw,NULL)==0);wait_atomic(&raw_acquired,1);return &raw_thread;}
static void settle(void){send_byte(resume_callback[1]);assert(pthread_join(callback_thread,NULL)==0);assert(atomic_load(gate())==2);send_byte(raw_resume[1]);assert(pthread_join(raw_thread,NULL)==0);rejected++;assert(callback(registration,101)==denied);rejected++;atomic_store_explicit(&drained,1,memory_order_release);}
void thread_request(void *token){assert(token==&raw_thread && requests++==0 && atomic_load(gate())==3);assert(pthread_equal(parent,pthread_self())==failed);assert(!buffer_frees && !reg_frees);atomic_store_explicit(&closing,1,memory_order_release);if(failed)settle();}
int32_t thread_drain(void *token){assert(token==&raw_thread && atomic_load(&closing)==1 && !buffer_frees && !reg_frees);assert(pthread_equal(parent,pthread_self())==failed);atomic_fetch_add_explicit(&polls,1,memory_order_release);return atomic_load_explicit(&drained,memory_order_acquire);}
int32_t thread_parent(void){assert(pthread_equal(parent,pthread_self()) && !failed && body_calls++==0);wait_atomic(&closing,1);assert(atomic_load(gate())==3 && atomic_load(&polls)==0 && !buffer_frees && !reg_frees);send_byte(resume_callback[1]);assert(pthread_join(callback_thread,NULL)==0);assert(atomic_load(gate())==2);struct timespec start,now;clock_gettime(CLOCK_MONOTONIC,&start);while(!atomic_load_explicit(&polls,memory_order_acquire)){sched_yield();clock_gettime(CLOCK_MONOTONIC,&now);assert(now.tv_sec-start.tv_sec<5);}assert(!buffer_frees && !reg_frees);send_byte(raw_resume[1]);assert(pthread_join(raw_thread,NULL)==0);rejected++;assert(callback(registration,101)==denied);rejected++;atomic_store_explicit(&drained,1,memory_order_release);return 9;}
int32_t thread_complete(int32_t code){assert(allocs==2 && task_frees==1 && reg_frees==1 && buffer_frees==1 && attempts==1 && requests==1 && atomic_load(&polls)>=1 && rejected==2);if(failed)assert(code==2 && !driver_started && !driver_joined && !body_calls);else assert(code==0 && driver_started==1 && driver_joined==1 && body_calls==1);puts("active frame moved; ClosingBusy and raw-holder drained; once cleanup");return code;}
