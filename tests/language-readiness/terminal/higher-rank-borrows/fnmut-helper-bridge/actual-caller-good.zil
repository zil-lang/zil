struct Tag<life s>{p:&'s i64}
fn helper<life s,F:FnMut(&Tag<'s>)->i32>(f:&mut F,x:&Tag<'s>)->i32{return f(x)}
fn apply<life a,life b>(left:&Tag<'a>,right:&Tag<'b>)->i32{let saved=right;var count=0;var f=move(p:&Tag<'a>)->i32=>{count+=1;return i32(*p.p+*left.p)+count-1};return helper(&mut f,left)}
fn setup<life a,life b>(x:&'a i64,y:&'b i64)->i32{let left:Tag<'a>=Tag::<'a>{p:x};let right:Tag<'b>=Tag::<'b>{p:y};return apply(&left,&right)}fn main()->i32{let x:i64=21;let y:i64=21;return setup(&x,&y)}
