extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}resource R{code:i32}impl Drop for R{fn drop(self:Self){emit(.code)}}
fn run<F:FnOnce()->i32>(f:F,n:i32)->i32{var total=0;for index in 0..n{total=f();break}return total}
fn main()->i32{let n=input();let held=R{code:72};let f=move()=>{emit(67);return held.code};let total=run(f,n);emit(90);if n>0{return total-72}return total}
