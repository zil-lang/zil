struct Tag<life s>{p:&'s i64}
resource Mark{n:i32}
fn dispatch<life s,F:FnOnce(&Tag<'s>)->i32,G:FnMut(&Tag<'s>)->i32>(f:F,g:&mut G,x:&Tag<'s>)->i32{
    var n=0;
    var total=0;
    while n<3{total=total+g(x);n=n+1}
    let moved=f;
    return moved(x)+total;
}
fn apply<life s>(left:&Tag<'s>)->i32{
    let mark=Mark{n:1};
    let f=move(p:&Tag<'s>)->i32=>{let taken=give mark;return i32(*p.p+*left.p)+taken.n-1};
    var state=0;
    var g=move(p:&Tag<'s>)->i32=>{let observed=*p.p+*left.p;observed;state=state+1;return state};
    return dispatch(f,&mut g,left);
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}
fn main()->i32{let x:i64=21;return setup(&x)}
