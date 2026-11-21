fn identity<T:Value>(value:T)->T{return value}
fn edit<life a,life b>(first:&'a mut i32,second:&'b mut i32){
 let pair=identity::<(&'a mut i32,&'b mut i32)>((first,second))
 var (left,right)=pair
 *left+=1
 *right+=2
}
fn main()->i32{var first:i32=20;var second:i32=19;edit(&mut first,&mut second);return first+second-42}
