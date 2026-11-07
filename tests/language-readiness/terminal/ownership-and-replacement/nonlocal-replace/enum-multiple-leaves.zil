extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum E{Left(manages R,manages R,i32),Right(i32,manages R,manages R),Empty}
fn reset(target:&mut E,left:bool){
    *target=if left{E.Left(R{code:66},R{code:67},42)}else{E.Right(43,R{code:68},R{code:69})};
}
fn work(left:bool)->i32{
    var value=E.Left(R{code:65},R{code:70},0);
    reset(&mut value,left);
    match value{
        E.Left(r,s,n)=>{return n+r.code+s.code-133},
        E.Right(n,r,s)=>{return n+r.code+s.code-137},
        E.Empty()=>{return 0}
    }
}
fn main()->i32{let a=work(true);let b=work(false);unsafe{putchar(48+a/10);putchar(48+a%10);putchar(44);putchar(48+b/10);putchar(48+b%10);putchar(10)}return 0}
