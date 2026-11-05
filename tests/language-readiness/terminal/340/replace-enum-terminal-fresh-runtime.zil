extern "C" fn putchar(code:i32)->i32;
extern "C" fn getchar()->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code);}}}
enum E{Left(manages R,i32),Right(i32,manages R),Empty}
fn fresh(input:E,left:bool)->R contract{result(create)}{
    var value=input;
    let target=&mut value;
    *target=if left{E.Left(R{code:66},1)}else{E.Right(2,R{code:68})};
    match value{
        E.Left(r,n)=>{return give r;},
        E.Right(n,r)=>{return give r;},
        E.Empty()=>{return R{code:90};}
    }
}
fn main()->i32{var selector=0;unsafe{selector=getchar();}let owner=fresh(E.Left(R{code:65},1),selector==76);return owner.code*10+2;}
