extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}
fn make()->impl FnMut()->&i32{var step=0;return move()=>{step+=1;return &step}}
fn run<F:FnMut()->&i32>(f:&mut F,n:i32)->i32{var total=0;var i=0;while i<n{let view=f();total+=*view;i+=1}return total}
fn main()->i32{let n=input();var f=make();let total=run(&mut f,n);let after=f();return total-n*(n+1)/2+*after-n-1}
