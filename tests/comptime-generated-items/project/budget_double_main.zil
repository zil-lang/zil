import generators as gen
fn main()->i32 {
    let first:#(gen.type_id(i32))=1;
    let second:#(gen.type_id(i32))=2;
    return first+second-3
}
