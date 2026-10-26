// Trusted Linux libc boundary: exact int(void)/int(int) C signatures; no
// borrowed storage, callbacks, unwinding, cancellation or resource transfer.
// getchar returns an unsigned byte or EOF (-1 on this target). EOF includes
// input errors in this bounded adapter; inspecting ferror needs pointer FFI.
// putchar reports errors observed by that call. A later buffered flush can fail.
extern "C" fn getchar() -> i32;
extern "C" fn putchar(value: i32) -> i32;

pub fn read() -> i32 {
    unsafe { return getchar() }
}

pub fn write(value: i32) -> bool {
    unsafe { return putchar(value) >= 0 }
}
