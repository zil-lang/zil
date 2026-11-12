fn identity<T:Value>(value:T)->T{return value}
fn edit<life a,life b,life c>(first:&'a mut i32,second:&'b mut i32,third:&'c mut i32){
 let tuple=identity::<((&'a mut i32,&'b mut i32),&'c mut i32)>(((first,second),third))
 var (pair,last)=tuple
 var (left,right)=pair
 *left+=1
 *right+=2
 *last+=3
}
fn main()->i32{var first:i32=10;var second:i32=18;var third:i32=8;edit(&mut first,&mut second,&mut third);return first+second+third-42}
