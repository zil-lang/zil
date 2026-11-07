import common.model as model
type Four=[i32;4]
pub fn get_x(value:model.Point)->i32 { return value.x }
fn main()->i32 {
    let scalar:i32=1;
    let values:Four=[scalar,2,3,4];
    let point=model.Point{x:5,y:8};
    if values[3]!=4 || get_x(point)!=5 { return 1 }
    if i64(8)!=8 { return 2 }
    if i64(4)!=4 { return 3 }
    return 0
}
