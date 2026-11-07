#include <stdint.h>
#include <stdlib.h>

int main(int argc, char **argv) {
    if (argc != 2) return 64;
    char *end = NULL;
    unsigned long long raw = strtoull(argv[1], &end, 10);
    if (!end || *end || raw > 64ULL * 1024ULL * 1024ULL) return 65;
    size_t bytes = (size_t)raw;
    volatile unsigned char *p = malloc(bytes);
    if (!p) return 66;
    for (size_t i = 0; i < bytes; i += 4096) p[i] = (unsigned char)(i / 4096);
    if (bytes) p[bytes - 1] = 7;
    free((void *)p);
    return 0;
}
