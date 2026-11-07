import common.io as io
import providers.support as library_support
type Selected=i64
fn main()->i32 {
    let typed:Selected=i64(7);
    let tmp=90;
    let first_generated_tmp=75;
    io.emit(first_generated_tmp);
    let second_generated_tmp=75;
    io.emit(second_generated_tmp);
    io.emit(tmp);
    if typed!=i64(7) { return 1 }
    return library_support.bump_runtime(41)-42
}
