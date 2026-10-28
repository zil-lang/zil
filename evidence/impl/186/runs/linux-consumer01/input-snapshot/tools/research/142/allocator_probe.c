/* Test-only linker shim. Tracks the runtime's actual realloc/free ABI. */
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
void *__real_realloc(void *, size_t);
void __real_free(void *);
struct Entry {void *p; size_t size;};
static struct Entry entries[128];
static size_t calls, allocated, frees, failures, live, peak_live, bytes_live, peak_bytes;
static size_t fail_at;
static int installed;
static void report(void) {
    fprintf(stderr,"{\"alloc_calls\":%zu,\"allocated\":%zu,\"frees\":%zu,\"failures\":%zu,\"live\":%zu,\"peak_live\":%zu,\"peak_bytes\":%zu}\n",calls,allocated,frees,failures,live,peak_live,peak_bytes);
    if(live) _Exit(111);
}
void probe_fail_next(void) {fail_at=calls+1;}
void *__wrap_realloc(void *old, size_t size) {
    if(!installed){
        installed=1;
        const char *value=getenv("ZIL_TEST_FAIL_AT");
        if(value)fail_at=(size_t)strtoull(value,NULL,10);
        if(atexit(report)) _Exit(112);
    }
    ++calls;
    size_t i=0;
    if(old){while(i<128 && entries[i].p!=old)++i;if(i==128)_Exit(114);}
    else{while(i<128 && entries[i].p)++i;if(i==128)_Exit(113);}
    if(size==0)_Exit(115); /* ByteBuffer frees explicitly; never realloc(...,0). */
    if(fail_at && calls==fail_at){++failures;return NULL;}
    size_t old_size=old?entries[i].size:0;
    int fresh=old==NULL;
    void *next=__real_realloc(old,size);
    if(!next){++failures;return NULL;}
    entries[i]=(struct Entry){next,size};bytes_live=bytes_live-old_size+size;
    if(fresh){++allocated;++live;}
    if(live>peak_live)peak_live=live;
    if(bytes_live>peak_bytes)peak_bytes=bytes_live;
    return next;
}
void __wrap_free(void *p) {
    if(!p)return;
    size_t i=0;while(i<128 && entries[i].p!=p)++i;if(i==128)_Exit(114);
    bytes_live-=entries[i].size;entries[i].p=NULL;--live;++frees;__real_free(p);
}
