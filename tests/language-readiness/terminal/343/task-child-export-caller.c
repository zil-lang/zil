// Real C caller and allocation observer; the task/children are generated ZIL.
#include <stdint.h>
#include <stdbool.h>
#include <stdlib.h>
extern void *__real_calloc(size_t,size_t);
extern void __real_free(void *);
static size_t calls,live,allocated_size;
void *__wrap_calloc(size_t count,size_t size) {
    calls++;
    allocated_size=count*size;
    if(getenv("ZIL343_TASK_EXPORT_ALLOC_FAIL")) return NULL;
    void *p=__real_calloc(count,size);
    if(p) live++;
    return p;
}
void __wrap_free(void *p) {
    if(p) {if(!live) abort();live--;}
    __real_free(p);
}
extern void *task_init(int32_t) __asm__("zil.sabi.task.init.2");
extern uint64_t task_poll(void *) __asm__("zil.sabi.task.poll.2");
extern void task_cancel(void *) __asm__("zil.sabi.task.request_cancel.2");
extern bool task_close(void *) __asm__("zil.sabi.task.poll_close.2");
extern void task_destroy(void *) __asm__("zil.sabi.task.finish_destroy.2");
static uint32_t tag(uint64_t answer) {return (uint32_t)(answer>>32);}
static int32_t payload(uint64_t answer) {return (int32_t)(uint32_t)answer;}
int32_t task_child_external(void) {
    uint64_t answer=task_poll(NULL);
    if(tag(answer)!=4 || payload(answer)!=0) return 1;
    task_cancel(NULL);
    if(!task_close(NULL)) return 2;
    task_destroy(NULL);
    void *frame=task_init(7);
    if(!frame) return 3;
    const char *expected=getenv("ZIL343_EXPECT_FRAME_SIZE");
    if(expected && allocated_size!=strtoull(expected,NULL,10)) return 4;
    for(int i=0;i<8;i++) {
        answer=task_poll(frame);
        if(tag(answer)!=0 || payload(answer)!=0 || calls!=1) return 5;
    }
    answer=task_poll(frame);
    if(tag(answer)!=1 || payload(answer)!=26 || calls!=1) return 6;
    task_cancel(frame);
    if(tag(task_poll(frame))!=2) return 7;
    if(!task_close(frame) || !task_close(frame)) return 8;
    if(tag(task_poll(frame))!=4) return 9;
    task_destroy(frame);
    for(int step=0;step<9;step++) {
        frame=task_init(7);
        if(!frame) return 10;
        for(int i=0;i<step;i++) if(tag(task_poll(frame))!=0) return 11;
        task_cancel(frame);task_cancel(frame);
        answer=task_poll(frame);
        if(tag(answer)!=3 || payload(answer)!=0) return 12;
        if(!task_close(frame)) return 13;
        task_destroy(frame);
        if(calls!=(size_t)step+2) return 14;
    }
    return live==0?0:15;
}
