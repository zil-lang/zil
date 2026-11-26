// Windows x64 monotonic clock and sleep helpers via Win32 kernel32.dll.
extern "C" fn QueryPerformanceCounter(counter: &mut i64) -> i32
contract { provides(noescape(counter)); effects(ffi); };
extern "C" fn QueryPerformanceFrequency(freq: &mut i64) -> i32
contract { provides(noescape(freq)); effects(ffi); };
extern "C" fn Sleep(ms: u32);

// Returns monotonic clock nanoseconds via QueryPerformanceCounter/Frequency.
pub fn monotonic_ns() -> u64 {
    var counter: i64 = 0
    var freq: i64 = 0
    var ok_c: i32 = 0
    var ok_f: i32 = 0
    unsafe {
        ok_c = QueryPerformanceCounter(&mut counter)
        ok_f = QueryPerformanceFrequency(&mut freq)
    }
    if ok_c == 0 || ok_f == 0 || counter < 0 || freq <= 0 {
        return 0u64
    }
    let c = u64(counter)
    let f = u64(freq)
    let sec = c / f
    let rem = c % f
    return sec * 1000000000u64 + (rem * 1000000000u64) / f
}

// Sleeps for at least ms milliseconds via Win32 Sleep(DWORD).
pub fn sleep_ms(ms: u64) {
    var remaining = ms
    let max_chunk: u64 = 4294967294u64
    while remaining > 0u64 {
        var step = remaining
        if step > max_chunk {
            step = max_chunk
        }
        unsafe { Sleep(u32(step)) }
        remaining -= step
    }
}
