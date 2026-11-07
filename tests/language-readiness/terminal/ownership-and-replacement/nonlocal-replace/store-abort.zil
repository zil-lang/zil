extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
enum F{Bad}
fn fail()->F!i32{return F.Bad()}
fn assemble(give spare:R,n:i32)->P{return P{owner:R{code:n},n:42}}
fn work()->F!i32{
    let items:Store<P>=store_new();
    if !store_reserve(items,1){return 1}
    match store_push(items,P{owner:R{code:65},n:0}){
        .ok(_)=>{},.err(_)=>{return 2}
    }
    let target=store_at_mut(items,0);
    *target=assemble(R{code:66},fail()?);
    return 3;
}
fn main()->i32{match work(){.ok(_)=>{return 1},.err(_)=>{unsafe{putchar(52);putchar(50);putchar(10)}return 0}}}
