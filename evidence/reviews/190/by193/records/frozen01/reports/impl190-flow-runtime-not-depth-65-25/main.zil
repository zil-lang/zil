extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var total=0;var i=0;while i<n{total+=f();i+=1}return total}
fn main()->i32{let n=input()*1000;let f=move()=>1;return run(&f,n)-n}
