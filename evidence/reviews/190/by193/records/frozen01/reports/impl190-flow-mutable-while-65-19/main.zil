extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
fn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32{var total=0;var i=0;while i<n{total+=f();i+=1}return total}
fn main()->i32{let n=input();var count=0;var f=move()=>{count+=1;emit(64+count);return count};let total=run(&mut f,n);let after=f();return total-n*(n+1)/2+after-n-1}
