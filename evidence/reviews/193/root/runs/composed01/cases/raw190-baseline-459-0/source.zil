
fn run<F:Fn()->i32>(f:&F,n:i32)->i32 {
    var left=n;var total=0;
    while left>0 { total+=f();left-=1 }
    return total
}
fn main()->i32{let f=move()=>7;return run(&f,3)-21}
