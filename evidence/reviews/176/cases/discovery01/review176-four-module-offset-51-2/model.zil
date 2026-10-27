
extern "C" fn putchar(x:i32)->i32;
pub resource Stamp{pub code:i32}
impl Drop for Stamp{fn drop(self:Self){unsafe{putchar(.code)}}}
pub package Scene<F>{pub lead:[i64;3];pub callback:F;manages stamp:Stamp;pub tail:u16;pub seen:i32}
pub fn wrap<F:FnMut(i32)->i32>(callback:F)->Scene<F>{
    return Scene::<F>{lead:[17i64,19i64,23i64],callback:callback,stamp:Stamp{code:84},tail:513u16,seen:0}
}
