struct Tag<life s>{p:&'s i64}
resource Mark{n:i32}
fn twice<life s,F:Fn(&Tag<'s>)->i32>(f:&F,x:&Tag<'s>)->i32{return f(x)+f(x)}
fn apply<life s>(left:&Tag<'s>)->i32{
    let mark=Mark{n:7};
    let f=move(p:&Tag<'s>)->i32=>{let observed=*p.p+*left.p;observed;return mark.n};
    return twice(&f,left);
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}
fn main()->i32{let x:i64=21;return setup(&x)}
