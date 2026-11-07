extern "C" fn putchar(value:i32)->i32;

pub resource Marker { pub code:i32 }
impl Drop for Marker { fn drop(self:Self) { unsafe { putchar(.code) } } }

pub package Leaf { manages data:ByteBuffer; manages marker:Marker }
pub package Pair { left:Leaf; right:Leaf; tag:i32 }
pub package Root { child:Pair; pub tail:i32 }

fn leaf(code:i32,size:i64)->void!Leaf {
    let marker=Marker{code:code}
    let data=bytes_new()
    if !bytes_reserve(data,size) { return () }
    return Leaf{data:give data,marker:give marker}
}

pub fn make(code:i32,size:i64)->void!Root {
    let left=leaf(code,size)?
    let right=leaf(code+1,size+1)?
    return Root{child:Pair{left:left,right:right,tag:code},tail:code+2}
}
