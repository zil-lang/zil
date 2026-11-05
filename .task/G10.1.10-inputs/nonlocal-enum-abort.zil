extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum E{Left(manages R,manages R,i32),Right(i32,manages R,manages R),Empty}
enum F{Bad}
fn fail()->F!i32{return F.Bad()}
fn assemble(give spare:R,n:i32)->E{return E.Right(n,R{code:68},R{code:69})}
fn reset(target:&mut E)->F!i32{*target=assemble(R{code:67},fail()?);return 1}
fn work()->i32{
    var value=E.Left(R{code:65},R{code:66},42);
    match reset(&mut value){
        .ok(_)=>{return 0},
        .err(_)=>{match value{
            E.Left(a,b,n)=>{return n+a.code+b.code-131},
            E.Right(n,a,b)=>{return 0},
            E.Empty()=>{return 0}
        }}
    }
}
fn main()->i32{let n=work();unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}return 0}
