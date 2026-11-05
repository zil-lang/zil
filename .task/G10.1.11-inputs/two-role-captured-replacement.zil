extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages first:R;manages second:R;n:i32}
fn dispatch<F:FnMut()->void,G:FnMut()->void>(f:&mut F,g:&mut G){
    var remaining=2;
    while remaining>0{f();remaining=remaining-1}
    g();
}
fn work()->i32{
    var left=P{first:R{code:65},second:R{code:66},n:0};
    var right=P{first:R{code:67},second:R{code:68},n:0};
    let left_target=&mut left;
    let right_target=&mut right;
    var f=()->void=>{*left_target=P{first:R{code:69},second:R{code:70},n:42}};
    var g=()->void=>{*right_target=P{first:R{code:71},second:R{code:72},n:49}};
    dispatch(&mut f,&mut g);
    return left.n+right.n;
}
fn main()->i32{let n=work();unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}return 0}
