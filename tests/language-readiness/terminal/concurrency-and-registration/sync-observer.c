/* Failure injection only. The source program and synchronization implementation
   are compiled ZIL; these wrappers observe actual native pthread calls. */
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
static int mode(void) { const char *value=getenv("ZIL343_SYNC_FAILURE"); return value?atoi(value):0; }
static void event(int code) { putchar(code); fflush(stdout); }
void sync_observe(int code) { event(code); }
extern int __real_pthread_mutex_init(pthread_mutex_t *,const pthread_mutexattr_t *);
extern int __real_pthread_mutex_lock(pthread_mutex_t *);
extern int __real_pthread_mutex_unlock(pthread_mutex_t *);
extern int __real_pthread_mutex_destroy(pthread_mutex_t *);
int __wrap_pthread_mutex_init(pthread_mutex_t *mutex,const pthread_mutexattr_t *attributes) {
    event('I'); return mode()==1?11:__real_pthread_mutex_init(mutex,attributes);
}
int __wrap_pthread_mutex_lock(pthread_mutex_t *mutex) {
    event('L'); return mode()==2?22:__real_pthread_mutex_lock(mutex);
}
int __wrap_pthread_mutex_unlock(pthread_mutex_t *mutex) {
    event('U'); return mode()==3?22:__real_pthread_mutex_unlock(mutex);
}
int __wrap_pthread_mutex_destroy(pthread_mutex_t *mutex) {
    event('X'); return mode()==4?22:__real_pthread_mutex_destroy(mutex);
}
