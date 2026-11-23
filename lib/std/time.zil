// Linux x86_64 POSIX monotonic clock and sleep helpers.
struct Timespec {
    tv_sec:i64;
    tv_nsec:i64
}

extern "C" fn clock_gettime(clk_id:i32,tp:&mut Timespec)->i32
contract {provides(noescape(tp));effects(ffi);};
extern "C" fn nanosleep(req:&Timespec,rem:&mut Timespec)->i32
contract {provides(noescape(req),noescape(rem));effects(ffi);};

// Returns monotonic clock nanoseconds via CLOCK_MONOTONIC (1).
pub fn monotonic_ns()->u64 {
    var ts=Timespec{tv_sec:0,tv_nsec:0}
    var status:i32=-1
    unsafe{status=clock_gettime(1,&mut ts)}
    if status!=0 || ts.tv_sec<0 || ts.tv_nsec<0 {return 0u64}
    return u64(ts.tv_sec)*1000000000u64+u64(ts.tv_nsec)
}

// Sleeps for at least ms milliseconds via POSIX nanosleep(2), retrying on EINTR.
pub fn sleep_ms(ms:u64) {
    var req=Timespec{tv_sec:i64(ms/1000u64),tv_nsec:i64((ms%1000u64)*1000000u64)}
    var rem=Timespec{tv_sec:0,tv_nsec:0}
    loop {
        var status:i32=0
        unsafe{status=nanosleep(&req,&mut rem)}
        if status==0 {break}
        if rem.tv_sec<0 || rem.tv_nsec<0 || (rem.tv_sec==0 && rem.tv_nsec==0) {break}
        req=rem
        rem=Timespec{tv_sec:0,tv_nsec:0}
    }
}
