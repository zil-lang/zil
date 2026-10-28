extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}
fn leaf<F:FnMut()->i32>(f:&mut F,n:i32)->i32{var total=0;var i=0;while i<n{total+=f();i+=1}return total}
fn outer<F:FnMut()->i32>(f:&mut F,n:i32)->i32{var total=0;var i=0;while i<n{total+=leaf(f,1);total+=leaf(f,2);i+=1}return total}
fn main()->i32{let n=input();var step=0;var f=move()=>{step+=1;return step};let total=outer(&mut f,n);return total-(3*n)*(3*n+1)/2+f()-3*n-1}
