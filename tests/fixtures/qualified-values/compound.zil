struct View<life a>{value:&'a i32}
fn identity<T:Value>(value:T)->T{return value}
fn sum<life a,life b>(first:&'a i32,second:&'b i32)->i32 {
 let output=identity::<(View<'a>,View<'b>)>((View::<'a>{value:first},View::<'b>{value:second}))
 let (left,right)=output
 return *left.value+*right.value
}
fn main()->i32{var left:i32=20;var right:i32=22;return sum(&left,&right)-42}
