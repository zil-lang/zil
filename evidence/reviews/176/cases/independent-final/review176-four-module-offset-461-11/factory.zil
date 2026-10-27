
import model
pub fn counter(seed:i32,mark:i32)->impl FnMut(i32)->i32{
    var count=seed;let witness=model.Stamp{code:mark};
    return move(delta:i32)=>{count+=delta;return count+witness.code-mark}
}
pub fn same_environment(seed:i32,mark:i32)->impl FnMut(i32)->i32{return counter(seed,mark)}
