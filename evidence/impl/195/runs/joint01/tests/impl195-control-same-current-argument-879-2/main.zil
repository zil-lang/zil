
fn run<F:Fn(&mut i32)->i32>(f:&F,value:&mut i32,n:i32)->i32{var total=0;for index in 0..n{if index==0{continue}total+=f(value);break}return total}
fn main()->i32{var value=1;let f=(p:&mut i32)=>{*p+=1;return value};return run(&f,&mut value,2)}
