fn compose<F:Fn(i32)->i32,G:Fn(i32)->i32>(map:&F,finish:&G,x:i32)->i32{return finish(map(x))}fn main()->i32{let map=move(v:i32)=>v+1;let finish=move(v:i32)=>v*2;return compose(&map,&finish,20)-42}
