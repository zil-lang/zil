extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
enum F{Bad}
fn make(ok:bool)->F!P{
    if ok{return P{owner:R{code:65},n:42}}
    return F.Bad()
}
fn fail()->F!i32{return F.Bad()}
fn prepare(give prepared:R,n:i32)->R contract{result(create)}{return R{code:n}}
fn work(ok:bool)->F!i32{
    var value=make(ok);
    let input=&mut value;
    match *input{
        .ok(ref mut p)=>{p.owner=prepare(R{code:66},fail()?);return p.n},
        .err(_)=>{return 0}
    }
}
fn result()->i32{
    match work(true){.ok(_)=>{return 0},.err(_)=>{return 42}}
}
