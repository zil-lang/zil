/* Foreign test fixture: successful bytes reach real OS descriptors. */
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <unistd.h>
static unsigned writes, reads, closes;
int test_mode(void) {
    const char *mode=getenv("ZIL_BUFFER_MODE");
    return mode?atoi(mode):0;
}
ssize_t __real_write(int,const void *,size_t);
ssize_t __real_read(int,void *,size_t);
int __real_close(int);
ssize_t __wrap_write(int fd,const void *buffer,size_t count) {
    ++writes;
    int mode=test_mode();
    if(mode==3) return (ssize_t)count+1;
    if(writes==2 && mode==1) return 0;
    if(writes==2 && mode==2) return -1;
    return __real_write(fd,buffer,count<3?count:3);
}
ssize_t __wrap_read(int fd,void *buffer,size_t count) {
    ++reads;
    return __real_read(fd,buffer,count<3?count:3);
}
int __wrap_close(int fd) {++closes;return __real_close(fd);}
__attribute__((destructor)) static void report(void) {
    fprintf(stderr,"closes=%u reads=%u writes=%u\n",closes,reads,writes);
    if(closes!=2 || reads!=4) _Exit(99);
}
