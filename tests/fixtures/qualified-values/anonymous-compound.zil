fn identity<T:Value>(value:T)->T{return value}
fn sum<life a,life b>(first:&'a i32,second:&'b i32)->i32{
 let pair=identity::<(&'a i32,&'b i32)>((first,second))
 let (left,right)=pair
 return *left+*right
}
fn main()->i32{var first:i32=20;var second:i32=22;return sum(&first,&second)-42}
