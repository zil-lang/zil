extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
fn run<F:Fn(i32)->i32>(f:&F,n:i32)->i32{var index=0;var total=0;while index<n{index+=1;if index==2{continue}if index==5{break}total+=f(index)}return total}
fn main()->i32{let n=input();let f=move(value:i32)=>{emit(48+value);return value};let total=run(&f,n);emit(65+total);return 0}
