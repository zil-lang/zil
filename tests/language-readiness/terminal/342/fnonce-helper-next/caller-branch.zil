struct Tag<life s>{p:&'s i64}
resource Mark{n:i32}
fn helper<life s,F:FnOnce(&Tag<'s>)->i32>(f:F,x:&Tag<'s>)->i32{return f(x)}
fn apply<life s>(left:&Tag<'s>)->i32{let mark=Mark{n:1};let f=move(p:&Tag<'s>)->i32=>{let taken=give mark;return i32(*p.p+*left.p)+taken.n-1};if true{return helper(f,left)}else{return 0}}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}fn main()->i32{let x:i64=21;return setup(&x)}
