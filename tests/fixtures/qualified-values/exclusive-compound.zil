struct View<life a>{value:&'a mut i32}
fn identity<T:Value>(value:T)->T{return value}
fn update<life a,life b>(first:&'a mut i32,second:&'b mut i32){
 let pair=identity::<(View<'a>,View<'b>)>((View::<'a>{value:first},View::<'b>{value:second}))
 var (left,right)=pair
 *left.value+=1
 *right.value+=2
}
fn main()->i32{var left:i32=20;var right:i32=19;update(&mut left,&mut right);return left+right-42}
