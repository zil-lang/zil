enum Envelope<T:Copy>{Data(f64,T),Fixed(u64|f64),Empty}
type Number=u64|f64;
fn pack(flag:bool)->Envelope<Number> {
    if flag {
        let value:Number=20.0;
        return Envelope::<Number>.Data(0.0,value)
    }
    let value:Number=22u64;
    return Envelope::<Number>.Data(0.0,value)
}
fn score(x:Envelope<Number>)->i32 {
    match x {
        Envelope.Data(scale,value)=>{
            match value {f64(n)=>{return i32(n+scale)},u64(n)=>{return i32(n)}}
        }
        Envelope.Fixed(value)=>{return 0}
        Envelope.Empty()=>{return 0}
    }
}
fn result()->i32{return score(pack(true))+score(pack(false))}
