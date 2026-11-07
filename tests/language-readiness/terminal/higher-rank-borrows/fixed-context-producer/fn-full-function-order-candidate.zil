struct Tag<life s>{p:&'s i64}
fn apply<T:Copy,#N:usize,life s>(left:&Tag<'s>)->i32{let f=(p:&Tag<'s>)->i32=>i32(*p.p+*left.p);return f(left)}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply::<i64,3usize>(&tag)}fn main()->i32{let x:i64=21;return setup(&x)}
