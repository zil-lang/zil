/* Test-only linker shim, not the language/library implementation. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
extern void *__real_realloc(void *, size_t);
extern void __real_free(void *);
static long calls, allocated, released, live, fail_at;
static int started;
static void report(void) {
    fprintf(stderr,"{\"allocator_calls\":%ld,\"allocated\":%ld,\"released\":%ld,\"live\":%ld,\"fail_at\":%ld}\n",calls,allocated,released,live,fail_at);
    if(live!=0) { fflush(stderr); _Exit(97); }
}
void *__wrap_realloc(void *old,size_t size) {
    if(!started) { started=1; const char *s=getenv("ZIL_TEST_FAIL_AT"); fail_at=s?strtol(s,0,10):0; if(atexit(report)!=0) _Exit(98); }
    ++calls;
    if(calls==fail_at) return NULL;
    void *next=__real_realloc(old,size);
    if(next && !old) { ++allocated; ++live; }
    return next;
}
void __wrap_free(void *ptr) {
    if(ptr) { ++released; --live; }
    __real_free(ptr);
}
