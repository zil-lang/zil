struct Tag<life a,life b>{value:i32}
fn make_tag<life a,life b>(left:&'a i32,right:&'b i32)->Tag<'a,'b>{
 return Tag::<'a,'b>{value:42}
}
fn identity<T:Value>(value:T)->T{return value}
fn relay<life a>(input:&'a i32)->i32{
 let original=make_tag(input,input)
 let forwarded=identity(original)
 return forwarded.value
}
fn main()->i32{
 let input=1
 return relay(&input)-42
}
