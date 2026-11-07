struct Tag<T:Copy,#N:usize,life s>{p:&'s T}
fn apply<life s>(left:&Tag<i64,3usize,'s>)->i32{return i32(*left.p+*left.p)}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<i64,3usize,'s>=Tag::<i64,3usize,'s>{p:x};return apply(&tag)}
fn main()->i32{let x:i64=21;return setup(&x)}
