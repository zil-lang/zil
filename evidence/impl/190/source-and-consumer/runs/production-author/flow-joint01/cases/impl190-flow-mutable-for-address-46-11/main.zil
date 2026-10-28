extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}
fn run<F:FnMut(&mut i32)->i32>(f:&mut F,p:&mut i32,n:i32)->i32{var total=0;for index in 0..n{total+=f(p)}return total}
fn main()->i32{let n=input();var step=0;var f=move(p:&mut i32)=>{step+=1;*p+=step;return *p};var value=20;let total=run(&mut f,&mut value,n);return total-20*n-n*(n+1)*(n+2)/6+value-20-n*(n+1)/2}
