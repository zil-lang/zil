fn inc(x:i64)->i64 => x+1;
fn double(x:i64)->i64 => x*2;
struct Stage { callback:fn(i64)->i64; weight:i64; }
fn result()->i64 {
    let apply=(f,x)=>f(x);
    let choose=(flag)=>if flag {inc}else{double};
    var state=Stage{callback:inc,weight:2};
    var step=move (x)=>{
        state.weight+=1;
        state.callback=if state.weight>3 {double}else{inc};
        state.callback(x)+state.weight
    };
    let a=apply(inc,5000000000);
    let b=choose(false)(3);
    let c=step(4);
    let d=step(4);
    let slots=[inc,double];
    let mapped=move (i,x)=>slots[i](x);
    return a+b+c+d+mapped(1,8)
}
fn main()->i32 { if result()==5000000043 { return 0 } return 1 }
