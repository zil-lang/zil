
fn leaf<F:FnMut(i32)->i32>(f:&mut F,n:i32)->i32=>f(n)
fn inner<F:FnMut(i32)->i32>(f:&mut F,n:i32)->i32{let first=leaf(f,n);return leaf(f,n+1)+first}
fn outer<F:FnMut(i32)->i32>(f:&mut F,n:i32)->i32{let first=inner(f,n);return leaf(f,n+2)+first}
fn main()->i32{var total=0;var f=move(n:i32)=>{total+=n;return total};if outer(&mut f,1)!=10{return 1};return f(0)-6}
