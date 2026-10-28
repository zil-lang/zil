
fn make(n:i32)->impl Fn()->&i32{return move()=>&n}
fn leaf<F:Fn()->&i32>(f:&F)->&i32=>f()
fn outer<F:Fn()->&i32>(f:&F)->&i32=>leaf(f)
fn main()->i32{let f=make(4);let view=outer(&f);return *view-4}
