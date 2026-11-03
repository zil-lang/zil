fn compute(value:i64,delta:i32)->i64{return value+i64(delta)}
pub trait Factory{
    fn handler(self:&Self)->fn(i64,i32)->i64{return compute}
}
pub fn handler<T:Factory>(value:&T)->fn(i64,i32)->i64{return value.handler()}
