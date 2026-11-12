fn make(value:&mut i32)->impl FnMut()->&i32{
 var calls=0
 return move()=>{calls+=1;return &*value}
}
fn view<F:FnMut()->&i32>(f:&mut F)->&i32{return f()}
fn relay<F:FnMut()->&i32>(f:&mut F)->&i32{return view(f)}
fn main()->i32{
 var value=21
 var f=make(&mut value)
 let first=*relay(&mut f)
 let pointer=relay(&mut f)
 let second=*pointer
 let third=*relay(&mut f)
 return first+second+third-63
}
