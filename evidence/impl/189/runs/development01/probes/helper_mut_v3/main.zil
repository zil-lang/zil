fn apply<F:Fn(&mut i32)->i32>(f:&F,p:&mut i32)->i32{return f(p)+f(p)} fn main()->i32{var n=3;let f=(p:&mut i32)->i32=>{*p+=1;return *p};return apply(&f,&mut n)-9}
