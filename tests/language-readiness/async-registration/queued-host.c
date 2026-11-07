#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <stdbool.h>
#ifdef ASYNC_EXTERNAL
extern bool task_close(void *) __asm__("zil.sabi.task.poll_close.0");
static void *active_task;
static int close_reentries;
#endif
static void close_reentry(void) {
#ifdef ASYNC_EXTERNAL
    if(active_task) {if(task_close(active_task)) abort();close_reentries++;}
#endif
}

typedef int64_t (*callback_t)(void *, int32_t);
struct event { callback_t callback; void *context; int queued; int closing; };
static struct event source;
static int installed, requests, polls, rejected, pumped, recursion;
static int allocations, frees, drops, seen_pending, seen_ready, result_drops, attempts;
void async_result_drop(int32_t code) {
    if(code!=17 || result_drops || (getenv("ZIL_ASYNC_RESULT") && drops!=1)) abort();
    result_drops++;
}
void async_seen(int32_t value) {
    if(value==0) seen_pending++;
    else if(value==17) seen_ready++;
    else abort();
}
void async_note(int32_t code) {
    if(code!=37 || source.queued || source.context || source.callback || drops) abort();
    close_reentry();
    if(getenv("ZIL_ASYNC_TRACE")) {putchar('D');fflush(stdout);}
    drops++;
}
static void *frames[16];
void *__real_calloc(size_t,size_t);
void __real_free(void *);
void *__wrap_calloc(size_t count,size_t size) {
    attempts++;
    const char *fail=getenv("ZIL_ASYNC_ALLOC_FAIL");
    if(fail && attempts==atoi(fail)) return NULL;
    void *p=__real_calloc(count,size);
    if(p) { if(allocations>=16) abort(); frames[allocations++]=p; }
    return p;
}
void __wrap_free(void *p) {
    if(p) for(int i=0;i<allocations;i++) if(frames[i]==p) {frames[i]=NULL;frees++;break;}
    if(p && getenv("ZIL_ASYNC_TRACE")) {putchar('F');fflush(stdout);}
    __real_free(p);
}
int32_t async_reenter(void) {
    if(recursion) return 0;
    recursion=1;
    int64_t result=source.callback(source.context,99);
    recursion=0;
    if(result!=(INT64_C(1)<<32)) return 1;
    rejected++;
    close_reentry();
    return 0;
}
void *async_register(callback_t callback,void *context) {
    if(installed) abort();
    source.callback=callback; source.context=context; installed=1;
    if(callback(context,2)!=40 || callback(context,3)!=42) abort();
    // This actual queued event retains the raw callback/userdata independently
    // of the ZIL owner; request_close cannot remove an already queued event.
    if(getenv("ZIL_ASYNC_INSTALL_FAIL")) {source.callback=NULL;source.context=NULL;return NULL;}
    if(getenv("ZIL_ASYNC_TRACE")) {puts("installed live=2 drops=0");fflush(stdout);}
    source.queued=1;
    return &source;
}
void async_request(void *token) {
    if(token!=&source || source.closing) abort();
    source.closing=1;requests++;
    close_reentry();
    // A backend may synchronously reenter the trampoline during close.
    if(source.callback(source.context,100)!=(INT64_C(1)<<32)) abort();
    rejected++;
}
int32_t async_drain(void *token) {
    if(token!=&source || !source.closing) abort();
    polls++;
    close_reentry();
    if(getenv("ZIL_ASYNC_INVALID_DRAIN")) {printf("invalid-provider live=%d drops=%d\n",allocations-frees,drops);fflush(stdout);return 2;}
    if(source.queued) return 0;
    // No queued, pre-trampoline, entered or backend userdata accesses remain.
    // Clear the actual backend's retained pointers before returning no-future.
    source.callback=NULL;source.context=NULL;
    return 1;
}
int32_t async_pump(void) {
    if(!source.queued || !source.closing || allocations-frees!=2) return 1;
    struct event event=source;
    if(event.callback(event.context,101)!=(INT64_C(1)<<32)) return 2;
    rejected++;source.queued=0;pumped++;
    return 0;
}
int32_t async_complete(void) {
    if(getenv("ZIL_ASYNC_RESULT") && result_drops!=1) return 22;
    if(getenv("ZIL_ASYNC_NORMAL") && (seen_pending!=2 || seen_ready!=1)) return 21;
    if(drops!=1 || requests!=1 || polls!=2 || rejected!=4 || pumped!=1 || allocations!=2 || frees!=2 || source.context || source.callback) return 20;
    puts("queued late entry rejected; no-future before two frees");
    return 0;
}

int32_t registration_phase_complete(void) {
    if (requests!=1 || polls!=2 || pumped!=1 || rejected!=2 ||
        seen_pending!=1 || seen_ready!=1 || drops!=1 ||
        allocations!=frees || allocations!=1 || source.queued ||
        source.context || source.callback) return 60;
    puts("pending retried; ready drained; registration dropped once");
    return 0;
}
