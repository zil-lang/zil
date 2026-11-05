extern "C" fn putchar(code:i32)->i32;
struct Tag<life s>{p:&'s i64}
resource Mark{code:i32,n:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
fn once<life s,F:FnOnce(&Tag<'s>)->i32>(f:F,x:&Tag<'s>)->i32{return f(x)}
fn shared<life s>(left:&Tag<'s>)->i32{
    let mark=Mark{code:65,n:7};
    let f=move(p:&Tag<'s>)->i32=>{let seen=*p.p+*left.p;seen;return mark.n};
    return once(f,left);
}
fn exclusive<life s>(left:&Tag<'s>)->i32{
    let mark=Mark{code:66,n:7};
    var count=0;
    var f=move(p:&Tag<'s>)->i32=>{let seen=*p.p+*left.p;seen;count=count+1;return mark.n+count};
    return once(f,left);
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return shared(&tag)+exclusive(&tag)}
fn main()->i32{let x:i64=21;let n=setup(&x);unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}return 0}
