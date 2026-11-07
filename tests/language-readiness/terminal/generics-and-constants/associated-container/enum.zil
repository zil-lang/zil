trait Read{type Item:Copy;}
struct Number{}
struct Flag{}
impl Read for Number{type Item=i32;}
impl Read for Flag{type Item=bool;}
enum Bucket<T:Read>{Value(T.Item)}
fn wrap<T:Read>(value:T.Item)->Bucket<T>{return Bucket::<T>.Value(value)}
fn main()->i32{let number=wrap::<Number>(42);let flag=wrap::<Flag>(true);match number{Bucket.Value(value)=>{if value!=42{return 1}}}match flag{Bucket.Value(value)=>{if !value{return 2}}}return 0}
