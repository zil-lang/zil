struct Tag<life s>{p:&'s i64}
resource Mark{code:i32}
fn dispatch<life s,F:FnOnce(&Tag<'s>)->i32,G:FnOnce(&Tag<'s>)->i32>(f:F,g:G,x:&Tag<'s>,invoke:bool)->i32{
    if invoke{return f(x)+g(x)}
    return 7;
}
fn apply<life s>(left:&Tag<'s>,invoke:bool)->i32{
    let first=Mark{code:65};
    let second=Mark{code:66};
    let f=move(p:&Tag<'s>)->i32=>{let taken=give first;return i32(*p.p+*left.p)+taken.code-65};
    let g=move(p:&Tag<'s>)->i32=>{let taken=give second;return i32(*p.p+*left.p)+taken.code-66};
    return dispatch(f,g,left,invoke);
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag,true)+apply(&tag,false)}
fn main()->i32{let x:i64=21;return setup(&x)}
