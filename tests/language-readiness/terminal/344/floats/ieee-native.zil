fn add(a:f32,b:f32)->f32{return a+b}
fn rounded()->f32{return 16777217.0}
fn ct_add(a:f64,b:f64)->f64{return a+b}
fn result()->i32{
    let a=add(1.5f32,2.25f32);
    let b:f64=0.1+0.2;
    let ct:f64=#(ct_add(0.1,0.2));
    let zero:f32=-0.0;
    let inf:f32=1.0f32/zero;
    let nan:f64=0.0/0.0;
    let tiny:f32=1.401298464324817070923729583289916131280e-45;
    let roundtrip:i64=i64(f64(i64(16777217)));
    if f32(255u8)==255.0f32 && u16(65535.0)==65535u16 && a==3.75f32 && b==ct && rounded()==16777216.0f32 && inf<0.0f32 && nan!=nan && !(nan==nan) && !(nan<1.0) && tiny>0.0f32 && tiny*0.5f32==0.0f32 && tiny+tiny==2.80259692864963414184745916657983226256e-45f32 && i64(-9223372036854775808.0)==-9223372036854775808 && roundtrip==i64(16777217) && i32(-0.0)==0 && f64(f32(i64(16777217)))==16777216.0 {return 344}
    return 0
}
