struct Tag<life s>{p:&'s i64}
fn helper<life s,F:FnMut(&Tag<'s>)->i32>(f:&mut F,x:&Tag<'s>)->i32{return f(x)}
fn apply<life s>(seed:&Tag<'s>)->i32{var count=0;var f=move(p:&Tag<'s>)->i32=>{count+=1;return i32(*p.p+*seed.p)+count-1};let first=helper(&f,seed);let second=helper(&f,seed);if first!=42{return -1}if second!=43{return -2}return first*100+second}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}fn main()->i32{let x:i64=21;return setup(&x)}
