/* Controlled foreign implementation: this is a fixture, not libc evidence.
 * All successful writes still go to the real OS. Error/zero/impossible counts
 * are injected deliberately, so the wrapper's progress guard is observable. */
#include <stddef.h>
#include <stdlib.h>
#include <sys/types.h>
extern ssize_t __real_write(int fd, const void *buffer, size_t count);
ssize_t __wrap_write(int fd, const void *buffer, size_t count) {
    static unsigned calls;
    const char *mode = getenv("ZIL_SHORT_WRITE_MODE");
    ++calls;
    if (mode && mode[0] == '3') return (ssize_t)count + 1;
    if (calls == 2 && mode && mode[0] == '1') return 0;
    if (calls == 2 && mode && mode[0] == '2') return -1;
    size_t limit = calls == 2 ? 2 : 3;
    return __real_write(fd, buffer, count < limit ? count : limit);
}
