#define ASYNC_EXTERNAL 1
#include "queued-host.c"
extern void *task_init(int32_t) __asm__("zil.sabi.task.init.0");
extern uint64_t task_poll(void *) __asm__("zil.sabi.task.poll.0");
extern void task_cancel(void *) __asm__("zil.sabi.task.request_cancel.0");
extern void task_destroy(void *) __asm__("zil.sabi.task.finish_destroy.0");
static uint32_t tag(uint64_t answer) {return (uint32_t)(answer>>32);}
static int32_t payload(uint64_t answer) {return (int32_t)(uint32_t)answer;}
int32_t async_external(void) {
    active_task=task_init(37);
    const char *failure=getenv("ZIL_ASYNC_ALLOC_FAIL");
    if(failure && atoi(failure)==1) {
        if(active_task || allocations || frees || drops || installed) return 30;
        puts("task allocation failure; no installed source");return 0;
    }
    if(!active_task) return 31;
    uint64_t first=task_poll(active_task);
    if((failure && atoi(failure)==2)||getenv("ZIL_ASYNC_INSTALL_FAIL")) {
        if(tag(first)!=1 || payload(first)!=13 || drops!=1 || source.queued || source.context || source.callback) return 32;
        if(!task_close(active_task)) return 33;
        task_destroy(active_task);active_task=NULL;
        if(allocations!=frees || requests || polls) return 34;
        puts("registration failure; owned environment dropped exactly once");return 0;
    }
    if(tag(first)!=0 || payload(first)!=0 || allocations-frees!=2 || drops) return 35;
    if(getenv("ZIL_ASYNC_EARLY_ASYNC_FINISH")) {puts("early-async live=2 drops=0");fflush(stdout);task_poll(active_task);return 41;}
    if(getenv("ZIL_ASYNC_EARLY_TASK_FINISH")) {puts("early-finish live=2 drops=0");fflush(stdout);task_destroy(active_task);return 36;}
    if(getenv("ZIL_ASYNC_EXPORT_RETURN")) {
        uint64_t second=task_poll(active_task);
        if(tag(second)!=0 || payload(second)!=0 || drops || allocations-frees!=2) return 42;
        if(async_pump()!=0) return 43;
        uint64_t ready=task_poll(active_task);
        if(tag(ready)!=1 || payload(ready)!=17 || drops!=1 || allocations-frees!=1) return 44;
        if(!task_close(active_task)) return 45;
        task_destroy(active_task);active_task=NULL;
        if(close_reentries!=6) return 46;
        return async_complete();
    }
    task_cancel(active_task);task_cancel(active_task);
    if(task_close(active_task)) return 37;
    if(async_pump()!=0) return 38;
    if(!task_close(active_task) || !task_close(active_task) || drops!=1 || allocations-frees!=1) return 39;
    task_destroy(active_task);active_task=NULL;
    if(close_reentries!=6) return 40;
    return async_complete();
}
