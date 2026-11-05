struct Tag<life s>{p:&'s i64}
fn helper<life s,F:Fn(&Tag<'s>)->i32>(f:&F,x:&Tag<'s>)->i32{return f(x)}
fn apply<life a,life b>(left:&Tag<'a>,right:&Tag<'b>)->i32{let f=(p:&Tag<'a>)->i32=>i32(*p.p+*left.p);let saved=right;return helper(&f,left)}
fn setup<life a,life b>(x:&'a i64,y:&'b i64)->i32{let left:Tag<'a>=Tag::<'a>{p:x};let right:Tag<'b>=Tag::<'b>{p:y};return apply(&left,&right)}fn main()->i32{let x:i64=21;let y:i64=21;return setup(&x,&y)}
