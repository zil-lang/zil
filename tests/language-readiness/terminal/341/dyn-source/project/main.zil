import a;import b;
fn left(x:&dyn a.Read)->i32{return x.read()}
fn right(x:&dyn b.Read)->i32{return x.read()}
fn main(){}
