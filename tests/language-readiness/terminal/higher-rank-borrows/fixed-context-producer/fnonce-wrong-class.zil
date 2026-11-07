struct Tag<life s>{p:&'s i64}
resource Mark{n:i32}
fn apply<life a,life b>(left:&Tag<'a>,right:&Tag<'b>)->i32{let mark=Mark{n:1};let f=move(p:&Tag<'a>)->i32=>{let taken=give mark;return i32(*p.p+*left.p)+taken.n-1};return f(right)}
fn setup<life a,life b>(x:&'a i64,y:&'b i64)->i32{let left:Tag<'a>=Tag::<'a>{p:x};let right:Tag<'b>=Tag::<'b>{p:y};return apply(&left,&right)}fn main()->i32{let x:i64=21;let y:i64=21;return setup(&x,&y)}
