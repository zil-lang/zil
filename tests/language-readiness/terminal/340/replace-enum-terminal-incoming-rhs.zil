extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code);}}}
enum E{Left(manages R,i32),Right(i32,manages R),Empty}
fn incoming(give input:R)->R contract{result(create)}{
    var value=E.Left(R{code:65},1);
    let target=&mut value;
    *target=E.Right(42,give input);
    match value{
        E.Left(r,n)=>{return give r;},
        E.Right(n,r)=>{return give r;},
        E.Empty()=>{return R{code:90};}
    }
}
fn main()->i32{let owner=incoming(R{code:66});return owner.code-24;}
