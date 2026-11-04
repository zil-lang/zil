#define _GNU_SOURCE
#include <signal.h>
#include <execinfo.h>
#include <stdlib.h>
#include <unistd.h>
static _Thread_local char stack[131072];
static void traced(int signal) { void *bt[512]; int n=backtrace(bt,512); backtrace_symbols_fd(bt,n,2); _exit(128+signal); }
void setup_abort_trace(void) { stack_t s={.ss_sp=stack,.ss_size=sizeof(stack)}; sigaltstack(&s,0); struct sigaction a={0}; a.sa_handler=traced; a.sa_flags=SA_ONSTACK; sigemptyset(&a.sa_mask); sigaction(SIGABRT,&a,0); }
