/* Test-only Windows heap accounting. The emitted LLVM is separately retained;
 * only four imported call names are redirected in the instrumented copy.
 * All successful allocation operations use the real Windows process heap.
 * No Windows SDK or CRT structure definitions are needed by this shim. */
typedef unsigned long long Size;
typedef unsigned int Dword;
typedef void *Handle;
__declspec(dllimport) void *HeapAlloc(Handle, Dword, Size);
__declspec(dllimport) void *HeapReAlloc(Handle, Dword, void *, Size);
__declspec(dllimport) int HeapFree(Handle, Dword, void *);
__declspec(dllimport) Dword GetEnvironmentVariableA(const char *, char *, Dword);
__declspec(dllimport) Handle GetStdHandle(Dword);
__declspec(dllimport) int WriteFile(Handle, const void *, Dword, Dword *, void *);
__declspec(dllimport) __declspec(noreturn) void ExitProcess(Dword);

struct Allocation { void *pointer; Size bytes; };
static struct Allocation slots[256];
static Size calls, allocated, frees, failures, live, live_bytes, peak, peak_bytes;
static Size fail_at, bad_operations;
static int configured;

static void configure(void) {
    if (configured) return;
    configured = 1;
    char value[32];
    Dword length = GetEnvironmentVariableA("ZIL_TEST_FAIL_AT", value, sizeof(value));
    if (length >= sizeof(value)) { bad_operations++; return; }
    for (Dword i = 0; i < length; i++) {
        if (value[i] < '0' || value[i] > '9') { bad_operations++; return; }
        fail_at = fail_at * 10 + (Size)(value[i] - '0');
    }
}
static int should_fail(void) {
    configure(); calls++;
    if (fail_at && calls == fail_at) { failures++; return 1; }
    return 0;
}
static void peaks(void) {
    if (live > peak) peak = live;
    if (live_bytes > peak_bytes) peak_bytes = live_bytes;
}
static int find(void *pointer) {
    for (int i = 0; i < 256; i++) if (slots[i].pointer == pointer) return i;
    return -1;
}
void *zil_probe_HeapAlloc(Handle heap, Dword flags, Size bytes) {
    if (flags || !bytes) bad_operations++;
    if (should_fail()) return (void *)0;
    void *pointer = HeapAlloc(heap, flags, bytes);
    if (!pointer) { failures++; return pointer; }
    int slot = find((void *)0);
    if (slot < 0) { bad_operations++; HeapFree(heap, 0, pointer); return (void *)0; }
    slots[slot].pointer = pointer; slots[slot].bytes = bytes;
    allocated++; live++; live_bytes += bytes; peaks();
    return pointer;
}
void *zil_probe_HeapReAlloc(Handle heap, Dword flags, void *old, Size bytes) {
    int slot = old ? find(old) : -1;
    if (flags || !bytes || slot < 0) { bad_operations++; return (void *)0; }
    if (should_fail()) return (void *)0;
    void *pointer = HeapReAlloc(heap, flags, old, bytes);
    if (!pointer) { failures++; return pointer; }
    live_bytes -= slots[slot].bytes;
    live_bytes += bytes;
    slots[slot].pointer = pointer; slots[slot].bytes = bytes; peaks();
    return pointer;
}
int zil_probe_HeapFree(Handle heap, Dword flags, void *pointer) {
    int slot = pointer ? find(pointer) : -1;
    if (flags || slot < 0) { bad_operations++; return 0; }
    int result = HeapFree(heap, flags, pointer);
    if (!result) { bad_operations++; return result; }
    live_bytes -= slots[slot].bytes;
    slots[slot].pointer = (void *)0; slots[slot].bytes = 0;
    live--; frees++;
    return result;
}
static char *text(char *out, const char *value) {
    while (*value) *out++ = *value++;
    return out;
}
static char *number(char *out, Size value) {
    char reversed[21]; int count = 0;
    do { reversed[count++] = (char)('0' + value % 10); value /= 10; } while (value);
    while (count) *out++ = reversed[--count];
    return out;
}
static char *field(char *out, const char *name, Size value) {
    out = text(out, name); return number(out, value);
}
__declspec(noreturn) void zil_probe_ExitProcess(Dword code) {
    char buffer[512]; char *end = buffer;
    end = field(end, "{\"alloc_calls\":", calls);
    end = field(end, ",\"allocated\":", allocated);
    end = field(end, ",\"frees\":", frees);
    end = field(end, ",\"failures\":", failures);
    end = field(end, ",\"live\":", live);
    end = field(end, ",\"peak_live\":", peak);
    end = field(end, ",\"peak_bytes\":", peak_bytes);
    end = field(end, ",\"bad_operations\":", bad_operations);
    end = text(end, "}\n");
    Dword written = 0;
    if (!WriteFile(GetStdHandle((Dword)-12), buffer, (Dword)(end-buffer), &written, (void *)0)
        || written != (Dword)(end-buffer)) ExitProcess(112);
    if (bad_operations || live || allocated != frees) ExitProcess(113);
    ExitProcess(code);
}
