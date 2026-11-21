struct Tag<life a>{code:i32}
fn identity<T:Value>(value:T)->T{return value}
fn read<life a,life b>(first:&'a i32,second:&'b i32)->i32{
 let tag=Tag::<'a>{code:*first}
 let pair=identity::<(Tag<'a>,&'b i32)>((tag,second))
 let (retained,view)=pair
 return retained.code+*view
}
fn main()->i32{var first:i32=20;var second:i32=22;return read(&first,&second)-42}
