extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}resource R{code:i32}impl Drop for R{fn drop(self:Self){emit(.code)}}
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var total=0;var i=0;while i<n{total+=f();i+=1}return total}
fn main()->i32{let n=input();let owned=R{code:72};let f=move()=>{emit(65);return owned.code};let total=run(&f,n);emit(90);return total-72*n}
