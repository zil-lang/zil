package Buffer { manages data:ByteBuffer }
fn view(value:&Buffer)->&[u8] {return bytes_view(value.data)}
fn id(n:i32)->i32{return n}
fn plus(n:i32)->i32{return n+1}
fn identity<T:Value>(value:T)->T{return value}
fn edit<life a,life b>(first:&'a mut i32,second:&'b mut i32,select:bool){
 let pair=identity::<(&'a mut i32,&'b mut i32)>((first,second))
 var (left,right)=pair
 var f:fn(i32)->i32=id
 if select{f=plus}
 *left=f(*left+*right)
}
fn main()->i32{
 let owner=Buffer{data:bytes_new()}
 if view(&owner).len()!=0{return 1}
 var a:i32=20
 var b:i32=21
 edit(&mut a,&mut b,true)
 return a-42
}
