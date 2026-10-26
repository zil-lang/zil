// Ordinary ZIL: decoding is not a compiler primitive or a foreign runtime.
pub struct Step { pub value:i64; pub next:i64; pub valid:bool }

fn bad(offset:i64)->Step {
    return Step{value:0,next:offset,valid:false}
}

pub fn decode(bytes:&[u8],offset:i64)->Step {
    if offset<0 || offset>=bytes.len(){return bad(offset)}
    let first=i64(bytes[offset]);
    if first<128{return Step{value:first,next:offset+1,valid:true}}
    var width:i64=0;
    var scalar:i64=0;
    if first>=194 && first<=223{width=2;scalar=first&31}
    else if first>=224 && first<=239{width=3;scalar=first&15}
    else if first>=240 && first<=244{width=4;scalar=first&7}
    else{return bad(offset)}
    // Subtract first, so even an extreme legal offset cannot overflow a sum.
    if bytes.len()-offset<width{return bad(offset)}
    var part:i64=1;
    while part<width{
        let byte=i64(bytes[offset+part]);
        if byte<128 || byte>191{return bad(offset)}
        scalar=(scalar<<6)|(byte&63);
        part+=1;
    }
    if (width==2 && scalar<128)||(width==3 && scalar<2048)||(width==4 && scalar<65536){return bad(offset)}
    if scalar>1114111 || (scalar>=55296 && scalar<=57343){return bad(offset)}
    return Step{value:scalar,next:offset+width,valid:true}
}
