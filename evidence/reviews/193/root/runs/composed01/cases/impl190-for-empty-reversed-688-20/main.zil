
fn visit<T>(token:T,low:i64,high:i64)->(T,i32){
    let index=7;var sum=0;
    for index in low..high{sum+=i32(index)}
    return (token,sum+index)
}
fn main()->i32{let empty=visit(0,i64(2),i64(2));let reversed=visit(0,i64(5),i64(2));return empty.1+reversed.1-14}
