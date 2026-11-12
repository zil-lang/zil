extern "C" fn putchar(value:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Mixed<life a>{manages owned:R;pointer:&'a i32}
fn wrap<life a>(pointer:&'a i32)->Mixed<'a>{return Mixed::<'a>{owned:R{code:65},pointer:pointer}}
fn make<life a>(holder:Mixed<'a>)->impl Fn()->i32{return move()=>holder.owned.code+*holder.pointer}
fn run(fail:bool)->void!i32{let n=1;let f=make(wrap(&n));if fail{return ()};return f()-66}
fn main()->i32{match run(true){.err(_)=>{},.ok(_)=>{return 1}};match run(false){.err(_)=>{return 2},.ok(value)=>{return value}}}
