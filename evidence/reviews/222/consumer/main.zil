extern "C" fn getchar()->i32;
extern "C" fn putchar(n:i32)->i32;
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
fn run(mode:i32)->void!i32{
    let root=Mark{code:90}
    #for(code in [65,66]){
        let mark=Mark{code:code}
        let bytes=bytes_new()
        if !bytes_reserve(bytes,i64(code-64)){return ()}
    }
    if mode==0{return 0}
    var turn=0
    while turn<2{
        turn+=1
        #for(code in [67,68]){
            let mark=Mark{code:code}
            if mode==1{if turn==1{continue}}
            if mode==2{if code==68{break}}
            if mode==3{return ()}
        }
    }
    return 0
}
fn main()->void!i32{
    var mode=0
    unsafe{mode=getchar()-48}
    if mode<0||mode>3{return 9}
    return run(mode)
}
