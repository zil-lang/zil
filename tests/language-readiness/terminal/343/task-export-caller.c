#include <stdint.h>
#include <stdbool.h>
#include <stdlib.h>
extern void *task_init(int32_t) __asm__("zil.sabi.task.init.0");
extern uint64_t task_poll(void *) __asm__("zil.sabi.task.poll.0");
extern void task_cancel(void *) __asm__("zil.sabi.task.request_cancel.0");
extern bool task_close(void *) __asm__("zil.sabi.task.poll_close.0");
extern void task_destroy(void *) __asm__("zil.sabi.task.finish_destroy.0");
static uint32_t tag(uint64_t answer) {return (uint32_t)(answer>>32);}
static int32_t payload(uint64_t answer) {return (int32_t)(uint32_t)answer;}
int32_t task_external(void) {
    uint64_t answer=task_poll(NULL);
    if(tag(answer)!=4 || payload(answer)!=0) return 1;
    task_cancel(NULL);
    if(!task_close(NULL)) return 2;
    task_destroy(NULL);
    void *frame=task_init(-7);
    if(!frame) return 3;
    answer=task_poll(frame);
    if(tag(answer)!=0 || payload(answer)!=0) return 4;
    answer=task_poll(frame);
    if(tag(answer)!=0 || payload(answer)!=0) return 5;
    answer=task_poll(frame);
    if(tag(answer)!=1 || payload(answer)!=-5) return 6;
    answer=task_poll(frame);
    if(tag(answer)!=2 || payload(answer)!=0) return 7;
    task_cancel(frame);
    if(tag(task_poll(frame))!=2) return 8;
    if(!task_close(frame) || !task_close(frame)) return 9;
    if(tag(task_poll(frame))!=4) return 10;
    task_destroy(frame);
    for(int step=0;step<3;step++) {
        frame=task_init(7);
        if(!frame) return 11;
        for(int i=0;i<step;i++) if(tag(task_poll(frame))!=0) return 12;
        task_cancel(frame);task_cancel(frame);
        answer=task_poll(frame);
        if(tag(answer)!=3 || payload(answer)!=0) return 13;
        if(!task_close(frame)) return 14;
        task_destroy(frame);
    }
    return 0;
}
