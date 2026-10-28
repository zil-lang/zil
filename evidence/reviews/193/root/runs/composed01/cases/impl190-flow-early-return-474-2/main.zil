extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
fn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32{var i=0;while i<n{let value=f();if value==2{return value}i+=1}return 0}
fn main()->i32{let n=input();var step=0;var f=move()=>{step+=1;emit(64+step);return step};let answer=run(&mut f,n);var expected=0;var calls=n;if n>=2{expected=2;calls=2}return answer-expected+f()-calls-1}
