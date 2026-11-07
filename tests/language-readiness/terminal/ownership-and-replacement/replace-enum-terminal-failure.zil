extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code);}}}
enum E{Left(manages R,i32),Right(i32,manages R),Empty}
enum F{Bad}
fn fail()->F!i32{return F.Bad();}
fn assemble(packet:E,n:i32)->E{return packet;}
fn work()->F!i32{
    var value=E.Left(R{code:65},1);
    let target=&mut value;
    *target=assemble(E.Right(42,R{code:66}),fail()?);
    match value{
        E.Left(r,n)=>{return n+r.code;},
        E.Right(n,r)=>{return n+r.code;},
        E.Empty()=>{return 99;}
    }
}
fn main()->i32{match work(){.ok(_)=>{return 0;},.err(_)=>{return 42;}}}
