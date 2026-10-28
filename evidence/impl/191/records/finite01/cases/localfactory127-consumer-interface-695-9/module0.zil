import combine
import io
resource End{code:i32}
impl Drop for End{fn drop(self:Self){io.emit(.code)}}

fn main()->bool!i32{
    let end=End{code:84};
    let f=move (n:i64)->bool!i64=>{
        let observed=end.code;
        io.emit(80);
        return n+1
    };
    let result=combine.run(&f,0)?;
    if result!=49{return true}
    io.emit(83);
    return 0
}
