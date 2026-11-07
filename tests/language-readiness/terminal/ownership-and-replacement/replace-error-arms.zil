extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
package Failure{manages owner:R;n:i32}
fn make_ok()->Failure!P{return P{owner:R{code:65},n:21}}
fn make_err()->Failure!P{return Failure{owner:R{code:67},n:21}}
fn work_ok()->i32{
    var value=make_ok();
    match value{
        .ok(ref mut p)=>{p.owner=R{code:66};return p.n},
        .err(_)=>{return 0}
    }
}
fn work_err()->i32{
    var value=make_err();
    match value{
        .ok(_)=>{return 0},
        .err(ref mut p)=>{p.owner=R{code:68};return p.n}
    }
}
fn result()->i32{return work_ok()+work_err()}
