extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var total=0;while i<n{total+=f();i+=1}return total}
fn main()->i32{let n=input();let f=move()=>{emit(65);return 3};return run(&f,n)-3*n}
