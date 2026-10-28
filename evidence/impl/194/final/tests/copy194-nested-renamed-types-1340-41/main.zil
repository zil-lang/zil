
fn leaf<U:Copy,G:Fn(U)->U>(g:&G,x:U)->U=>g(x)
fn outer<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>leaf(f,x)
fn main()->i32{let f=move(n:i32)=>n+1;return outer(&f,6)-7}
