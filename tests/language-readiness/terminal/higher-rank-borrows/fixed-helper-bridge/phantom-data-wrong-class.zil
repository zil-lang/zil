struct Tag<life s>{p:&'s i64}
struct Phantom<life s>{n:i32}
fn helper<life s,F:Fn(&Phantom<'s>)->i32>(f:&F,x:&Phantom<'s>)->i32{return f(x)}
fn apply<life a,life b>(seed:&Tag<'a>)->i32{let f=(p:&Phantom<'a>)->i32=>p.n+i32(*seed.p);let wrong:Phantom<'b>=Phantom::<'b>{n:21};return helper(&f,&wrong)}
fn setup<life a,life b>(x:&'a i64,y:&'b i64)->i32{let seed:Tag<'a>=Tag::<'a>{p:x};return apply(&seed)}fn main()->i32{let x:i64=21;let y:i64=0;return setup(&x,&y)}
