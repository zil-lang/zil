extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}resource R{code:i32}impl Drop for R{fn drop(self:Self){emit(.code)}}
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var total=0;for index in 0..n{let guard=R{code:48+index};if index==0{continue}if index==3{break}total+=f()}return total}
fn main()->i32{let n=input();let owned=R{code:72};let f=move()=>{emit(67);return owned.code};let total=run(&f,n);emit(90);var calls=0;if n>1{calls=1}if n>2{calls=2}return total-72*calls}
