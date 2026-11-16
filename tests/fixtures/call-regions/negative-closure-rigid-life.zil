fn bad<life a,life b>(left:&'a i32,right:&'b i32) {
    let consume=(argument:&'a i32)=>{};
    consume(right)
}
fn main()->i32{return 0}
