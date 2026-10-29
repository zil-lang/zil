extern "C" fn putchar(n:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}enum E{Fail}
fn leaf<F:Fn()->E!i32>(f:&F)->E!i32=>f()
fn inner<F:Fn()->E!i32>(f:&F)->E!i32{let guard=R{code:66};let a=leaf(f)?;return leaf(f)?+a}
fn outer<F:Fn()->E!i32>(f:&F)->E!i32{let guard=R{code:67};return inner(f)?}
fn main()->E!i32{let held=R{code:65};let f=move()->E!i32=>{let observed=held.code;if true{return E.Fail};return 7};return outer(&f)?-14}
