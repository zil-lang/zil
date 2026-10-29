extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
fn leaf<F:Fn()->i32>(f:&F,n:i32)->i32{var total=0;for index in 0..n{if index==0{continue}if index==3{break}total+=f()}return total}
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var left=n;var total=0;while left>0{left-=1;if left==1{continue}total+=leaf(f,5);if left==0{break}}return total}
fn main()->i32{let n=input();let f=move()=>{emit(65);return 3};var count=n;if n>=2{count=n-1}return run(&f,n)-6*count}
