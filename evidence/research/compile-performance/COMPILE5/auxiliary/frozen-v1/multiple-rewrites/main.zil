import work
extern "C" fn putchar(x:i32)->i32;
fn main()->i32 {
    let result=work.run();
    unsafe { putchar(i32(result%251)); }
    return 0;
}
