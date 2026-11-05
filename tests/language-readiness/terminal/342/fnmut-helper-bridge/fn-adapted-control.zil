struct Tag<life s>{p:&'s i64}
fn helper<life s,F:FnMut(&Tag<'s>)->i32>(f:&mut F,x:&Tag<'s>)->i32{return f(x)}
fn apply<life s>(seed:&Tag<'s>)->i32{var f=(p:&Tag<'s>)->i32=>i32(*p.p+*seed.p);return helper(&mut f,seed)}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}fn main()->i32{let x:i64=21;return setup(&x)}
