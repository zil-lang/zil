extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
fn run<F:Fn(i32)->i32>(f:&F,n:i32)->i32{var total=0;for index in 0..n{total+=f(index)}return total}
fn main()->i32{let n=input();let f=move(value:i32)=>{emit(65+value);return value+1};return run(&f,n)-n*(n+1)/2}
