import std.float
extern "C" fn probe_float_begin(rounding:i32,pollution:i32)->i32;
extern "C" fn probe_float64(value:f64,index:i32)->i32;
extern "C" fn probe_float32(value:f32,index:i32)->i32;
extern "C" fn probe_float_end()->i32;
fn check64(text:&[u8],index:i32,mode:i32,seed:i32,range:bool)->i32 {
    var ok:i32=0
    unsafe{ok=probe_float_begin(mode,seed)}
    if ok!=1 {return 1}
    match float.parse_f64(text) {
        .err(error)=>{if !range {return 2};match error{float.Error.Range()=>{},_=>{return 3}}},
        .ok(value)=>{if range {return 4};unsafe{ok=probe_float64(value,index)};if ok!=1 {return 5}}
    }
    unsafe{ok=probe_float_end()}
    if ok!=1 {return 6}
    return 0
}
fn check32(text:&[u8],index:i32,mode:i32,seed:i32,range:bool)->i32 {
    var ok:i32=0
    unsafe{ok=probe_float_begin(mode,seed)}
    if ok!=1 {return 1}
    match float.parse_f32(text) {
        .err(error)=>{if !range {return 2};match error{float.Error.Range()=>{},_=>{return 3}}},
        .ok(value)=>{if range {return 4};unsafe{ok=probe_float32(value,index)};if ok!=1 {return 5}}
    }
    unsafe{ok=probe_float_end()}
    if ok!=1 {return 6}
    return 0
}
fn main()->i32 {
    var mode:i32=0
    while mode<4 {
        var seed:i32=0
        while seed<3 {
            if check64("1.7976931348623157e308",0,mode,seed,false)!=0 {return 10}
            if check64("-1.7976931348623157e308",1,mode,seed,false)!=0 {return 11}
            if check64("5e-324",2,mode,seed,false)!=0 {return 12}
            if check64("-5e-324",3,mode,seed,false)!=0 {return 13}
            if check64("1.7976931348623155e308",4,mode,seed,false)!=0 {return 14}
            if check64("-1.7976931348623155e308",5,mode,seed,false)!=0 {return 15}
            if check64("1e-99999",6,mode,seed,mode!=2)!=0 {return 16}
            if check64("-1e-99999",7,mode,seed,mode!=1)!=0 {return 17}
            if check64("0",8,mode,seed,false)!=0 {return 18}
            if check64("-0",9,mode,seed,false)!=0 {return 19}
            if check64("1e309",0,mode,seed,true)!=0 {return 60}
            if check64("-1e309",0,mode,seed,true)!=0 {return 61}
            if check32("3.4028234663852885e38",0,mode,seed,false)!=0 {return 40}
            if check32("-3.4028234663852885e38",1,mode,seed,false)!=0 {return 41}
            if check32("1.5e-45",2,mode,seed,false)!=0 {return 42}
            if check32("-1.5e-45",3,mode,seed,false)!=0 {return 43}
            if check32("3.4028232e38",4,mode,seed,false)!=0 {return 44}
            if check32("-3.4028232e38",5,mode,seed,false)!=0 {return 45}
            if check32("1e-99999",6,mode,seed,mode!=2)!=0 {return 46}
            if check32("-1e-99999",7,mode,seed,mode!=1)!=0 {return 47}
            if check32("0",8,mode,seed,false)!=0 {return 48}
            if check32("-0",9,mode,seed,false)!=0 {return 49}
            if check32("3.5e38",0,mode,seed,true)!=0 {return 70}
            if check32("-3.5e38",0,mode,seed,true)!=0 {return 71}
            var ok:i32=0
            unsafe{ok=probe_float_begin(mode,seed)}
            if ok!=1 {return 81}
            match float.format_f64(1.25) {.err(_)=>{return 82},.ok(text)=>{if text.len==0{return 83}}}
            match float.format_f32(1.25f32) {.err(_)=>{return 84},.ok(text)=>{if text.len==0{return 85}}}
            unsafe{ok=probe_float_end()}
            if ok!=1 {return 86}
            seed+=1
        }
        mode+=1
    }
    return 0
}
