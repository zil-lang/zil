fn exceptional(t:Type,x:f64)->f64{if t==f64{return x/0.0}return -0.0}
fn small(t:Type)->f32{if t==f32{return 1.401298464324817070923729583289916131280e-45f32}return -0.0f32}
fn result()->i32{
    let inf=#(exceptional(f64,1.0));
    let nan=#(exceptional(f64,0.0));
    let zero=#(exceptional(i32,0.0));
    let tiny=#(small(f32));
    if inf>1.0 && nan!=nan && 1.0/zero<0.0 && tiny==1.401298464324817070923729583289916131280e-45f32{return 344}
    return 0
}
