import a;import b;
fn left(x:&dyn a.Read)->i32{return x.read()}
fn right(x:&dyn b.Read)->i32{return x.read()}
fn main()->i32{
    let left_value=a.Meter{n:20};
    let right_value=b.Meter{n:22};
    return left(&left_value)+right(&right_value)-29
}
