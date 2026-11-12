fn identity<T:Value>(value:T)->T{return value}
fn sum<life a,life b,life c>(first:&'a i32,second:&'b i32,third:&'c i32)->i32{
 let triple=identity::<((&'a i32,&'b i32),&'c i32)>(((first,second),third))
 let (pair,last)=triple
 let (left,right)=pair
 return *left+*right+*last
}
fn main()->i32{var first:i32=10;var second:i32=20;var third:i32=12;return sum(&first,&second,&third)-42}
