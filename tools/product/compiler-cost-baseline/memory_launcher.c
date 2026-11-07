#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc < 4 || strcmp(argv[2], "--") != 0) return 64;
    pid_t child = fork();
    if (child < 0) { perror("fork"); return 70; }
    if (child == 0) {
        execv(argv[3], &argv[3]);
        perror("execv");
        _exit(127);
    }
    int status = 0;
    struct rusage usage;
    pid_t result;
    do { result = wait4(child, &status, 0, &usage); } while (result < 0 && errno == EINTR);
    if (result < 0) { perror("wait4"); return 70; }
    FILE *out = fopen(argv[1], "wx");
    if (!out) { perror("create receipt"); return 73; }
    int code = WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
    int signal_number = WIFSIGNALED(status) ? WTERMSIG(status) : 0;
    int written = fprintf(out,
        "{\"exit\":%d,\"signal\":%d,\"peak_rss_kib\":%ld,"
        "\"user_seconds\":%.6f,\"system_seconds\":%.6f}\n",
        code, signal_number, usage.ru_maxrss,
        (double)usage.ru_utime.tv_sec + (double)usage.ru_utime.tv_usec / 1000000.0,
        (double)usage.ru_stime.tv_sec + (double)usage.ru_stime.tv_usec / 1000000.0);
    int closed = fclose(out);
    if (written < 0 || closed != 0) return 74;
    return code;
}
