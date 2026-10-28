extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}resource R{code:i32}impl Drop for R{fn drop(self:Self){emit(.code)}}enum E{Fail}
fn run<F:FnMut()->E!i32>(f:&mut F,n:i32)->E!i32{let guard=R{code:67};var i=0;var total=0;while i<n{let per=R{code:66};let value=f()?;total+=value;i+=1}return total}
fn main()->E!i32{let n=input();let held=R{code:65};var step=0;var f=move()->E!i32=>{held.code;step+=1;if step==2{return E.Fail}return 1};return run(&mut f,n)?-n}
