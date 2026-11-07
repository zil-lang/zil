struct Pair{a:f32,b:f64}
extern "C" fn consume(a:f32,b:f64,p:&Pair)->f64 contract{provides(noescape(p));effects(ffi);};
fn inspect(a:f32,b:f64)->f64{let pair=Pair{a,b};unsafe{return consume(a,b,&pair)}}
fn rounded(a:f32)->f32{return a+1.0f32}
