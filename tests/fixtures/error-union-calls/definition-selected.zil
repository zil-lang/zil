// The symbolic definition chooses each arm before U becomes i32.
fn value_first<T:Copy>(value:T!i32,witness:T)->i32 {
    match value{.ok(number)=>{return number},.err(_)=>{return 19}}
}
fn witness_first<T:Copy>(witness:T,value:T!i32)->i32 {
    match value{.ok(number)=>{return number},.err(_)=>{return 19}}
}
fn first_success<U:Copy>(witness:U)->i32{return value_first(7,witness)}
fn second_success<U:Copy>(witness:U)->i32{return witness_first(witness,7)}
fn first_failure<U:Copy>(witness:U)->i32{return value_first(witness,witness)}
fn second_failure<U:Copy>(witness:U)->i32{return witness_first(witness,witness)}
fn main()->i32 {
    if first_success(0)!=7{return 1}
    if second_success(0)!=7{return 2}
    if first_failure(0)!=19{return 3}
    if second_failure(0)!=19{return 4}
    let success=error_ok::<i32,i32>(41)
    let failure=error_err::<i32,i32>(5)
    if value_first(success,0)!=41{return 5}
    if value_first(failure,0)!=19{return 6}
    return 0
}
