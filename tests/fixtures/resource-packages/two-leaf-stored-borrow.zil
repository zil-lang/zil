struct Pair<life a,life b>{left:&'a mut i32,right:&'b mut i32}
fn hold<life a,life b>(left:&'a mut i32,right:&'b mut i32)->Pair<'a,'b>{return Pair::<'a,'b>{left:left,right:right}}
fn bump<life a,life b>(pair:&mut Pair<'a,'b>){let left=pair.left;let right=pair.right;*left=*left+1;*right=*right+2}
fn main()->i32{var left=20;var right=19;if true{var pair=hold(&mut left,&mut right);bump(&mut pair)}return left+right}
