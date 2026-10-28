
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}

fn run<T:Copy,F:Fn(T)->T>(f:&F,x:T,fail:bool)->bool!T{let local=R{code:66};if fail{return false}return f(x)}
fn main()->bool!i32{let owner=R{code:65};let f=move(n:i32)=>{owner.code;return n};return run(&f,7,false)?-7}
