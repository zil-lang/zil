extern "C" fn putchar(code:i32)->i32;
struct Tag<life s>{p:&'s i64}
resource Mark{code:i32,n:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
fn twice<life s,F:Fn(&Tag<'s>)->i32>(f:&F,x:&Tag<'s>)->i32{return f(x)+f(x)}
fn twice_mut<life s,F:FnMut(&Tag<'s>)->i32>(f:&mut F,x:&Tag<'s>)->i32{return f(x)+f(x)}
fn peek_mut<life s,life env,F:FnMut(&Tag<'s>)->&i32>(f:&'env mut F,x:&Tag<'s>)->&'env i32{return f(x)}
fn shared<life s>(left:&Tag<'s>)->i32{
    let mark=Mark{code:65,n:7};
    let f=move(p:&Tag<'s>)->i32=>{let observed=*p.p+*left.p;observed;return mark.n};
    return twice(&f,left);
}
fn exclusive<life s>(left:&Tag<'s>)->i32{
    let mark=Mark{code:66,n:7};
    var value=0;
    var f=move(p:&Tag<'s>)->i32=>{let observed=*p.p+*left.p;observed;value=value+1;return mark.n+value};
    return twice_mut(&mut f,left);
}
fn lending<life s>(left:&Tag<'s>)->i32{
    let mark=Mark{code:67,n:7};
    var value=0;
    var f=move(p:&Tag<'s>)=>{let observed=*p.p+*left.p;observed;value=value+mark.n;return &value};
    let a=peek_mut(&mut f,left);
    let old=*a;
    let b=peek_mut(&mut f,left);
    return old+*b;
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return shared(&tag)+exclusive(&tag)+lending(&tag)}
fn main()->i32{let x:i64=21;let n=setup(&x);unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}return 0}
