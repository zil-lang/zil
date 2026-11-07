import common.io as io
import providers.support as library_support
pub fn from_provider()->i32 { return 7 }
fn next(count:&mut i32)->i32 { *count+=1; return 65 }
fn main()->i32 {
    var count=0;
    let tmp=90;
    let support=999;
    let generated_tmp=next(&mut count);
    library_support.emit(generated_tmp);
    library_support.emit(generated_tmp);
    io.emit(tmp);
    let value=library_support.bump_runtime(41);
    let scaled=21*2;
    let values:[i32;4]=[1,2,3,4];
    if count!=1 || tmp!=90 || support!=999 { return 1 }
    if value!=42 || scaled!=42 || values[3]!=4 || from_provider()!=7 { return 2 }
    io.emit(48+value/10);
    io.emit(48+value%10);
    return 0
}
