extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
fn leaf<F:Fn()->i32>(f:&F)->i32{let a=f();return a+f()}
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var total=0;var i=0;while i<n{total+=leaf(f);i+=1}return total}
fn outer<F:Fn()->i32>(f:&F,n:i32)->i32{let before=leaf(f);let middle=run(f,n);return before+middle+leaf(f)}
fn main()->i32{let n=input();let f=move()=>{emit(65);return 3};return outer(&f,n)-3*(4+2*n)}
