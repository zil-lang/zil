struct Tag<life s>{p:&'s i64}
resource Mark{n:i32}
fn helper<life s,F:FnOnce(&Tag<'s>)->i32>(flag:bool,f:F,x:&Tag<'s>)->i32{if flag{return f(x)+1}else{return f(x)+2}}
fn apply<life s>(left:&Tag<'s>,flag:bool,amount:i32)->i32{let mark=Mark{n:amount};let f=move(p:&Tag<'s>)->i32=>{let taken=give mark;return i32(*p.p+*left.p)+taken.n};return helper(flag,f,left)}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag,true,5)+apply(&tag,false,9)}
fn main()->i32{let x:i64=21;return setup(&x)}
