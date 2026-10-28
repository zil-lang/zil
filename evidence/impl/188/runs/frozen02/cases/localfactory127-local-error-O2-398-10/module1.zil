import factory

pub fn run<F:Fn(i64)->bool!i64>(f:&F,stop:i64)->bool!i64{
    var first=factory.make(10,65,stop);
    var second=factory.make(20,66,0);
    let unused=factory.make(0,67,0);
    let left=first(2)?;
    let right=second(3)?;
    let value=f(left+right)?;
    return value+first(1)?
}
