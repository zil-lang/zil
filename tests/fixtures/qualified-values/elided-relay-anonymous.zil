fn identity<T:Value>(value:T)->T{return value}
fn relay(value:&i32)->&i32{return value}
fn sum(first:&i32,second:&i32)->i32{
 let pair=identity((relay(first),second))
 let (left,right)=pair
 return *left+*right
}
fn main()->i32{var first:i32=20;var second:i32=22;return sum(&first,&second)-42}
