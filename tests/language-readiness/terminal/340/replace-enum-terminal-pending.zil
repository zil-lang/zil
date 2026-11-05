extern "C" fn putchar(code:i32)->i32;
extern "C" fn getchar()->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code);}}}
enum E{Left(manages R,i32),Right(i32,manages R),Empty}
fn initial(left:bool)->E{
    if left{return E.Left(R{code:65},1);}
    return E.Right(2,R{code:67});
}
fn main()->i32{
    var input=0;
    unsafe{input=getchar();}
    let left=input==76;
    var value=initial(left);
    let target=&mut value;
    *target=if left{E.Right(2,R{code:66})}else{E.Left(R{code:68},1)};
    match value{
        E.Left(r,n)=>{return n*100+r.code;},
        E.Right(n,r)=>{return n*100+r.code;},
        E.Empty()=>{return 99;}
    }
}
