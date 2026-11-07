struct Tag<life s>{p:&'s i64}
fn apply<life a,life b>(left:&Tag<'a>,right:&Tag<'b>)->i32{let f=(p:&Tag<'a>)->i32=>i32(*p.p+*left.p);return f(right)}
fn setup<life a,life b>(x:&'a i64,y:&'b i64)->i32{let left:Tag<'a>=Tag::<'a>{p:x};let right:Tag<'b>=Tag::<'b>{p:y};return apply(&left,&right)}fn main()->i32{let x:i64=21;let y:i64=21;return setup(&x,&y)}
