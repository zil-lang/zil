extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code);}}}
enum E{Left(manages R,i32),Right(i32,manages R),Empty}
fn main()->i32{
    var value=E.Left(R{code:65},1);
    let neighbor=R{code:67};
    let target=&mut value;
    *target=E.Right(2,R{code:66});
    match value{
        E.Left(ref r,n)=>{return n*100+r.code;},
        E.Right(n,ref r)=>{return n*100+r.code;},
        E.Empty()=>{return 99;}
    }
}
