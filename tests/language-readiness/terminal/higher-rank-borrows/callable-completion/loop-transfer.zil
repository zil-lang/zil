struct Tag<life s>{p:&'s i64}
resource Mark{n:i32}
fn helper<life s,F:FnOnce(&Tag<'s>)->i32>(f:F,x:&Tag<'s>)->i32{let g=f;var n:i32=0;while n<3{var k:i32=0;while k<2{k=k+1};n=n+1};let h=g;return h(x)+n}
fn apply<life s>(left:&Tag<'s>)->i32{let mark=Mark{n:1};let f=move(p:&Tag<'s>)->i32=>{let taken=give mark;return i32(*p.p+*left.p)+taken.n-1};return helper(f,left)}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}fn main()->i32{let x:i64=21;return setup(&x)}
