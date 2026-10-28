extern "C" fn putchar(n:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn eat(give r:R)->i32=>7
fn leaf<F:FnOnce()->i32>(f:F,invoke:bool)->i32{if invoke{return f()};return 0}
fn inner<F:FnOnce()->i32>(f:F,invoke:bool)->i32=>leaf(f,invoke)
fn outer<F:FnOnce()->i32>(f:F,invoke:bool)->i32=>inner(f,invoke)
fn main()->i32{let r=R{code:65};let f=move()=>eat(give r);return outer(f,false)-0}
