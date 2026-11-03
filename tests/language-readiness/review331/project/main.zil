import api
fn main()->i32{
    let value:#(api.selected_type(7))=40;
    var other:i32=2;
    let selected=api.first::<i32>(&value,&other);
    other=9;
    return *selected+other-49
}
