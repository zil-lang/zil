struct Tag<life s>{p:&'s i64}
fn mark(trace:&mut i32,digit:i32)->i32{*trace*=10;*trace+=digit;return 0}
fn helper<life s,F:Fn(&Tag<'s>)->i32>(a:i32,x:&Tag<'s>,b:i32,f:&F,c:i32)->i32{return f(x)+a+b+c}
fn apply<life s>(seed:&Tag<'s>)->i32{var trace=0;let f=(p:&Tag<'s>)->i32=>i32(*p.p+*seed.p);let result=helper(mark(&mut trace,1),seed,mark(&mut trace,2),&f,mark(&mut trace,3));if trace!=123{return 0}return result}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}fn main()->i32{let x:i64=21;return setup(&x)}
