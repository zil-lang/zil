#define _GNU_SOURCE
#include <llvm-c/Transforms/PassBuilder.h>
#include <llvm-c/Core.h>
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static atomic_uint sequence;

static void fail(const char *message) {
  fprintf(stderr, "[zil-cgu-capture] failure: %s\n", message);
  _exit(125);
}

static void save(const char *directory, unsigned id, const char *suffix,
                 const char *text) {
  char path[PATH_MAX];
  int count = snprintf(path, sizeof(path), "%s/module-%u.%s", directory, id, suffix);
  if (count < 0 || (size_t)count >= sizeof(path)) fail("capture path too long");
  int fd = open(path, O_WRONLY | O_CREAT | O_EXCL, 0600);
  if (fd < 0) fail("refuse missing directory or existing capture");
  size_t remaining = strlen(text);
  while (remaining) {
    ssize_t written = write(fd, text, remaining);
    if (written < 0 && errno == EINTR) continue;
    if (written <= 0) fail("capture write failed");
    text += written;
    remaining -= (size_t)written;
  }
  if (close(fd)) fail("capture close failed");
}

LLVMErrorRef LLVMRunPasses(LLVMModuleRef module, const char *passes,
                         LLVMTargetMachineRef machine,
                         LLVMPassBuilderOptionsRef options) {
  typedef LLVMErrorRef (*Run)(LLVMModuleRef, const char *, LLVMTargetMachineRef,
                             LLVMPassBuilderOptionsRef);
  typedef char *(*Print)(LLVMModuleRef);
  typedef void (*Dispose)(char *);
  Run original = (Run)dlsym(RTLD_NEXT, "LLVMRunPasses");
  Print print_module = (Print)dlsym(RTLD_NEXT, "LLVMPrintModuleToString");
  Dispose dispose = (Dispose)dlsym(RTLD_NEXT, "LLVMDisposeMessage");
  if (!original || !print_module || !dispose) fail("missing original C API");
  const char *directory = getenv("ZIL_CGU_CAPTURE_DIR");
  const char *expected_executable = getenv("ZIL_CGU_CAPTURE_EXECUTABLE");
  const char *expected_library = getenv("ZIL_CGU_CAPTURE_LLVM");
  char executable[PATH_MAX], library[PATH_MAX];
  Dl_info loaded;
  if (!directory || !expected_executable || !expected_library ||
      !realpath("/proc/self/exe", executable) ||
      strcmp(executable, expected_executable) ||
      !dladdr((void *)original, &loaded) ||
      !realpath(loaded.dli_fname, library) || strcmp(library, expected_library))
    fail("compiler/library identity mismatch");
  unsigned id = atomic_fetch_add(&sequence, 1);
  char *before = print_module(module);
  if (!before) fail("LLVMPrintModuleToString returned null");
  save(directory, id, "before.ll", before);
  dispose(before);
  // No option, linkage, attribute, module or argument is modified by this shim.
  LLVMErrorRef result = original(module, passes, machine, options);
  char *after = print_module(module);
  if (!after) fail("LLVMPrintModuleToString returned null after pass execution");
  save(directory, id, "after.ll", after);
  dispose(after);
  char metadata[PATH_MAX + 1024];
  int count = snprintf(metadata, sizeof(metadata),
      "pid=%ld\nlibrary=%s\npasses=%s\nerror=%d\n",
      (long)getpid(), library, passes, result != NULL);
  if (count < 0 || (size_t)count >= sizeof(metadata)) fail("metadata too long");
  save(directory, id, "meta.txt", metadata);
  return result;
}
