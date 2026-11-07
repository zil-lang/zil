extern "C" fn putchar(n:i32)->i32;
resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}
enum E{V(i32,manages R)}
fn main()->i32{
 let early=R{n:69}
 defer{unsafe{putchar(81)}}
 let later=R{n:76}
 match E.V(R{n:88}.n,R{n:65}){
 E.V(88,r)=>{}
 E.V(_,_)=>{}
 }
 return 0
}
