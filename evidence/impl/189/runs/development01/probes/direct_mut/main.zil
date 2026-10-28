fn main()->i32 { var n=3; let f=(p:&mut i32)=>{*p+=1}; f(&mut n); return n-4 }
