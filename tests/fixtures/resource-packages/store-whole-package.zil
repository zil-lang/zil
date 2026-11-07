extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn work()->i32{
    let items:Store<P>=store_new();
    if !store_reserve(items,1){return 1}
    match store_push(items,P{owner:R{code:65},n:0}){
        .ok(_)=>{},.err(_)=>{return 2}
    }
    let target=store_at_mut(items,0);
    *target=P{owner:R{code:66},n:42};
    return target.n;
}
fn main()->i32{let n=work();unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}return 0}
