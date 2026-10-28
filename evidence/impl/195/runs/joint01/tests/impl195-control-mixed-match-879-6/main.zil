extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}resource R{code:i32}impl Drop for R{fn drop(self:Self){emit(.code)}}
fn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32{var total=0;for index in 0..n{let guard=R{code:48+index};match index==0{true=>{if n==1{return f()}else{continue}},false=>{if index==3{break}total+=f()}}}return total}
fn main()->i32{let n=input();var count=0;var f=move()=>{count+=1;emit(64+count);return count};let total=run(&mut f,n);emit(65+total);return 0}
