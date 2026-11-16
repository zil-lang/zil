struct Stamp<life a,T:Copy,#N:i32> { n:i32 }
fn unused<T:Copy,#N:i32,life a>(marker:Type)->Type {
    return Stamp::<'a,T,N>
}
fn main()->i32 { return 43 }
