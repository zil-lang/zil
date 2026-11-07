fn step(value:i32)->i32 { return value+2 }
pub fn compute(value:i32)->i32 {
    var result=0
    var index=0
    while index<3 { result+=step(value);index+=1 }
    return result
}
