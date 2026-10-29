fn apply<F:Fn(i32)->i32,G:Fn(i32)->i32>(f:&F,g:&G,x:i32)->i32=>g(f(x));fn main()->i32{let f=move(n:i32)=>n+1;let g=move(n:i32)=>n*2;return apply(&f,&g,3)-8}
