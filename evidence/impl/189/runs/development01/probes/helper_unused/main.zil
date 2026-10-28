fn apply<F:Fn(&i32)->i32>(f:&F,p:&i32)->i32{return f(p)} fn main()->i32{let n=3;let f=(p:&i32)->i32=>3;return apply(&f,&n)-3}
