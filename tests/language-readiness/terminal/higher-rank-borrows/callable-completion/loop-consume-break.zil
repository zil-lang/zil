extern "C" fn putchar(code:i32)->i32;
struct Tag<life s>{p:&'s i64}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
fn helper<life s,F:FnOnce(&Tag<'s>)->i32>(f:F,x:&Tag<'s>,count:i32)->i32{
    var n=0;
    var result=0;
    while n<count{
        if n==1{result=f(x);break}
        n=n+1;
    }
    return result;
}
fn apply<life s>(left:&Tag<'s>,count:i32,code:i32)->i32{
    let mark=Mark{code:code};
    let f=move(p:&Tag<'s>)->i32=>{let taken=give mark;let value=i32(*p.p+*left.p);taken.code;return value};
    return helper(f,left,count);
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag,3,65)+apply(&tag,0,66)}
fn main()->i32{let x:i64=21;let n=setup(&x);unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}return 0}
