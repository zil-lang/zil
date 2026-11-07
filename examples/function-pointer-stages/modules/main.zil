import ops
fn add(x:i64)->i64=>x+100
fn apply(f:fn(i64)->i64,x:i64)->i64=>f(x)
fn result()->i64 {
    var callback=ops.add
    let first=callback(5)
    callback=add
    let second=apply(callback,1)
    let factory=ops.choose
    return first*1000+second*10+factory(false)(3)
}
