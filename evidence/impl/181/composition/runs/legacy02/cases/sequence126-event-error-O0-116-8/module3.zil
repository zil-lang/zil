extern "C" fn putchar(n:i32)->i32;pub fn emit(n:i32){unsafe{putchar(n)}}
