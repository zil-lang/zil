fn main()->i32{var x=7;let f=(p:&mut i32)=>{*p=8};f(&mut x);return x-8}
