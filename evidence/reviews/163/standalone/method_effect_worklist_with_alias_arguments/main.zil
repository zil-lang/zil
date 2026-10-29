resource R{n:i32}
impl R(self:Self){fn update(other:R){other.n+=1};fn read()->i32=>.n}
fn step0(a:R,b:R){step1(a,b)}
fn step1(a:R,b:R){step2(a,b)}
fn step2(a:R,b:R){step3(a,b)}
fn step3(a:R,b:R){step4(a,b)}
fn step4(a:R,b:R){step5(a,b)}
fn step5(a:R,b:R){step6(a,b)}
fn step6(a:R,b:R){step7(a,b)}
fn step7(a:R,b:R){step8(a,b)}
fn step8(a:R,b:R){step9(a,b)}
fn step9(a:R,b:R){step10(a,b)}
fn step10(a:R,b:R){step11(a,b)}
fn step11(a:R,b:R){step12(a,b)}
fn step12(a:R,b:R){step13(a,b)}
fn step13(a:R,b:R){step14(a,b)}
fn step14(a:R,b:R){step15(a,b)}
fn step15(a:R,b:R){step16(a,b)}
fn step16(a:R,b:R){step17(a,b)}
fn step17(a:R,b:R){let alias=a;alias.update(b)}
fn make()->impl FnMut()->i32{let a=R{n:0};let b=R{n:40};return move()=>{step0(a,b);return a.read()+b.read()}}
fn main()->i32{var f=make();if f()!=41||f()!=42{return 1};return 0}
