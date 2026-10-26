/* Test-only allocator observation. No byte operations or ownership decisions. */
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
extern void *__real_malloc(size_t);
extern void *__real_realloc(void *, size_t);
extern void __real_free(void *);
static struct { void *pointer; unsigned id; int live; } slots[64];
static unsigned next_id, attempts, live;
static int find(void *p) {
    for(unsigned i=0;i<next_id;i++) if(slots[i].live && slots[i].pointer==p) return (int)i;
    fputs("observer: unknown or repeated release\n",stderr);_Exit(93);
}
static void *allocate(void *old,size_t size,int use_realloc) {
    int index=old?find(old):-1;
    unsigned id=index<0?0:slots[index].id;
    attempts++;
    if(attempts==ROAM_FAIL_AT) { printf("F%02X%02X;",id,(unsigned)size);return NULL; }
    void *next=use_realloc?__real_realloc(old,size):__real_malloc(size);
    if(!next) { fputs("observer: unplanned allocation failure\n",stderr);_Exit(95); }
    if(index<0) {
        if(next_id>=64) { fputs("observer: identity quota\n",stderr);_Exit(94); }
        index=(int)next_id++;id=next_id;slots[index].id=id;slots[index].live=1;live++;
        printf("A%02X%02X;",id,(unsigned)size);
    } else printf("G%02X%02X;",id,(unsigned)size);
    slots[index].pointer=next;return next;
}
void *__wrap_malloc(size_t n) { return allocate(NULL,n,0); }
void *__wrap_realloc(void *p,size_t n) { return allocate(p,n,1); }
void __wrap_free(void *p) {
    if(!p)return;
    int i=find(p);slots[i].live=0;live--;printf("D%02X;",slots[i].id);__real_free(p);
}
int roam_byte_finish(void) { printf("L%02X;",live);return (int)live; }
