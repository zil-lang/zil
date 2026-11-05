extern "C" fn putchar(code:i32)->i32;
struct Tag<life s>{p:&'s i64}
resource Mark{n:i32,code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
fn helper<life s,F:FnOnce(&Tag<'s>)->i32>(f:F,x:&Tag<'s>,give spare:Mark)->i32{return f(x)+spare.n}
fn apply<life s>(left:&Tag<'s>)->i32{let mark=Mark{n:1,code:65};let f=move(p:&Tag<'s>)->i32=>{let taken=give mark;return i32(*p.p+*left.p)+taken.n-1};let spare=Mark{n:3,code:83};return helper(f,left,give spare)}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}
fn main()->i32{let x:i64=21;let n=setup(&x);unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}return 0}
