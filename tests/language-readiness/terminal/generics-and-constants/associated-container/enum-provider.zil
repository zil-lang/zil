trait Read<Tag>{type Item:Copy;}
struct P{}
impl Read<i32> for P{type Item=i32;}
impl Read<bool> for P{type Item=bool;}
enum Bucket<T:Read<i32>>{Value(T.Item)}
fn wrap<T:Read<i32>>(value:T.Item)->Bucket<T>{return Bucket::<T>.Value(value)}
fn main()->i32{match wrap::<P>(42){Bucket.Value(value)=>{return value-42}}}
