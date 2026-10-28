
struct Bounds{high:i64}
fn identity<T>(value:T)->T=>value
fn sum<T>(token:T,bounds:Bounds,items:[i64;1])->(T,i32){
    var sum=0;
    for index in 0..(bounds.high+1){sum+=i32(index)}
    for index in 0..items[0]{sum+=i32(index)}
    for index in 0..identity(bounds.high){sum+=i32(index)}
    return (token,sum)
}
fn main()->i32{let result=sum(0,Bounds{high:2},[i64(3)]);return result.0+result.1-7}
