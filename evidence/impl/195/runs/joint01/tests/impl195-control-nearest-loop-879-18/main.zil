extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
fn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32{var outer=0;var total=0;while outer<n{outer+=1;for inner in 0..4{if inner==0{continue}if inner==2{break}total+=f()}}return total}
fn main()->i32{let n=input();var calls=0;var f=move()=>{calls+=1;emit(64+calls);return calls};return run(&mut f,n)-n*(n+1)/2}
