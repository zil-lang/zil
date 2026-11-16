struct Stamp<life a,T:Copy,#N:i32> { n:i32 }
fn bad<T:Copy,#N:i32,life a>(marker:Type)->Type {
    return Stamp::<T,'a,N>
}
fn main()->i32 { return 0 }
