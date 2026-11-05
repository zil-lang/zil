struct Tag<life s>{p:&'s i64}
fn apply<life s>(left:&Tag<'s>)->i32{return i32(*left.p+*left.p)}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}
fn main()->i32{let x:i64=21;return setup(&x)}
