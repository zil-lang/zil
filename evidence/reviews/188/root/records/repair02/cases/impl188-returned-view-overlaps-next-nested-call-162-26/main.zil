fn make(n:i32)->impl FnMut()->&i32{var value=n;return move()=>{value+=1;return &value}}
fn leaf<F:FnMut()->&i32>(f:&mut F)->&i32=>f()
fn run<F:FnMut()->&i32>(f:&mut F)->&i32=>leaf(f)
fn main()->i32{var f=make(7);let view=run(&mut f);let later=run(&mut f);return *view+*later}