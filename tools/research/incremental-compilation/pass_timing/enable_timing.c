#define _GNU_SOURCE
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* Research constructor only. No LLVM symbol is interposed, no LLVM DSO is
 * linked into otherwise unrelated child tools, and no pass pipeline is edited.
 * Signatures are the llvmorg-23.1.4 public C API (Support.h / Core.h). */
typedef void (*ParseOptions)(int, const char *const *, const char *);
typedef void (*GetVersion)(unsigned *, unsigned *, unsigned *);
typedef void (*Shutdown)(void);
static Shutdown shutdown_llvm;

static void flush_on_exit(void) {
    /* The target CLI has returned and every per-call owned LLVM handle was
     * already released. LLVM's ManagedStatic timer groups print on shutdown.
     * This is research-process cleanup, never a production pipeline change. */
    fprintf(stderr, "[zil-api-pass-timing] flush managed timers at process exit\n");
    shutdown_llvm();
}

static void fail(const char *message) {
    fprintf(stderr, "[zil-api-pass-timing] refusal: %s\n", message);
    _exit(78);
}

__attribute__((constructor)) static void enable_timing(void) {
    const char *target = getenv("ZIL_PASS_TIMING_TARGET");
    if (!target || !*target)
        return;
    char executable[PATH_MAX + 1];
    ssize_t length = readlink("/proc/self/exe", executable, PATH_MAX);
    if (length < 0 || length == PATH_MAX)
        return;
    executable[length] = '\0';
    if (strcmp(target, executable) != 0)
        return;

    const char *expected_library = getenv("ZIL_PASS_TIMING_LLVM");
    if (!expected_library || !*expected_library)
        fail("expected LLVM library missing");
    void *parse_symbol = dlsym(RTLD_DEFAULT, "LLVMParseCommandLineOptions");
    void *version_symbol = dlsym(RTLD_DEFAULT, "LLVMGetVersion");
    void *shutdown_symbol = dlsym(RTLD_DEFAULT, "LLVMShutdown");
    if (!parse_symbol || !version_symbol || !shutdown_symbol)
        fail("required existing LLVM C API symbol missing");
    Dl_info info;
    char actual_library[PATH_MAX + 1];
    if (!dladdr(parse_symbol, &info) || !info.dli_fname ||
        !realpath(info.dli_fname, actual_library) ||
        strcmp(expected_library, actual_library) != 0)
        fail("LLVM options API resolved outside the recorded library");
    if (!dladdr(version_symbol, &info) || !info.dli_fname ||
        !realpath(info.dli_fname, actual_library) ||
        strcmp(expected_library, actual_library) != 0)
        fail("LLVM version API resolved outside the recorded library");
    if (!dladdr(shutdown_symbol, &info) || !info.dli_fname ||
        !realpath(info.dli_fname, actual_library) ||
        strcmp(expected_library, actual_library) != 0)
        fail("LLVM shutdown API resolved outside the recorded library");
    unsigned major = 0, minor = 0, patch = 0;
    ((GetVersion)version_symbol)(&major, &minor, &patch);
    if (major != 23 || minor != 1 || patch != 4)
        fail("LLVM version is not 23.1.4");
    const char *slp = getenv("ZIL_PASS_TIMING_SLP");
    const char *arguments[] = {"zil-api-pass-timing", "-time-passes", NULL};
    int argument_count = 2;
    if (slp && *slp) {
        if (strcmp(slp, "on") == 0)
            arguments[argument_count++] = "-vectorize-slp=true";
        else if (strcmp(slp, "off") == 0)
            arguments[argument_count++] = "-vectorize-slp=false";
        else
            fail("SLP selection must be on or off");
    }
    ((ParseOptions)parse_symbol)(argument_count, arguments,
                                "Isolated ZIL API pass diagnostic");
    shutdown_llvm = (Shutdown)shutdown_symbol;
    if (atexit(flush_on_exit) != 0)
        fail("cannot register timer cleanup");
    fprintf(stderr, "[zil-api-pass-timing] enabled llvm=%u.%u.%u library=%s\n",
            major, minor, patch, actual_library);
    if (slp && *slp)
        fprintf(stderr, "[zil-api-pass-timing] vectorize-slp=%s\n", slp);
}
