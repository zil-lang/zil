fn bad<life a,life b>(p:&'a mut i32,q:&'b mut i32,flag:bool)->&'a mut i32{if flag{return p}return q}
fn main()->i32{return 0}
