struct Tag<life s>{p:&'s i64}struct Phantom<life s>{n:i32}
resource Mark{n:i32}
fn helper<life s,F:FnOnce(&Phantom<'s>)->i32>(f:F,x:&Phantom<'s>)->i32{return f(x)}
fn apply<life a,life b>(left:&Tag<'a>,other:&'b i64)->i32{let mark=Mark{n:1};let f=move(p:&Phantom<'a>)->i32=>{let taken=give mark;return p.n+i32(*left.p)+taken.n-1};let data:Phantom<'b>=Phantom::<'b>{n:21};return helper(f,&data)}
fn setup<life a,life b>(x:&'a i64,y:&'b i64)->i32{let tag:Tag<'a>=Tag::<'a>{p:x};return apply(&tag,y)}fn main()->i32{let x:i64=21;let y:i64=21;return setup(&x,&y)}
