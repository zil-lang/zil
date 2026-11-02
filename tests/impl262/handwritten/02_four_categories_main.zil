import common.io as io
pub fn answer()->i32 { return 42 }
fn main()->i32 {
    let tmp=90;
    let first_generated_tmp=75;
    io.emit(first_generated_tmp);
    let second_generated_tmp=75;
    io.emit(second_generated_tmp);
    io.emit(tmp);
    let value=41+1;
    let values:[i32;2]=[value,answer()];
    io.emit(48+values[0]/10);
    io.emit(48+values[0]%10);
    return values[1]-42
}
