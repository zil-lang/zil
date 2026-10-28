
extern "C" fn getchar()->i32;
extern "C" fn putchar(n:i32)->i32;
fn emit(n:i32){unsafe{putchar(n)}}
resource R{code:i32}
impl Drop for R{fn drop(self:Self){emit(.code)}}
fn piece<F:FnMut(i32)->i32>(f:&mut F,n:i32,tag:i32)->i32{
    var sum=0;for index in 0..n{sum+=f(tag+index)}return sum
}
fn route<F:FnMut(i32)->i32>(f:&mut F,n:i32)->i32{
    var sum=0;for index in 0..n{
        sum+=piece(f,index,65);sum+=f(120);sum+=piece(f,n-index,97)
    }return sum
}
fn main()->i32{
    var n=0;unsafe{n=getchar()};n-=48;
    let owned=R{code:72};var step=0;
    var callback=move(value:i32)=>{owned.code;step+=1;emit(value);return step};
    let answer=route(&mut callback,n);let calls=n*(n+1);let after=callback(90);
    return answer-calls*(calls+1)/2+after-calls-1
}
