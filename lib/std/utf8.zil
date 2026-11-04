// UTF-8 scalar validation. No allocation, normalization or grapheme segmentation.
pub struct Bounds { pub first:i64; pub last:i64; pub count:i64; pub error:i64 }

fn next(bytes:&[u8], at:i64)->i64 {
    let first=i64(bytes[at])
    if first<128 { return at+1 }
    var width:i64=0
    if first>=194 && first<=223 { width=2 }
    else if first>=224 && first<=239 { width=3 }
    else if first>=240 && first<=244 { width=4 }
    else { return -1 }
    if bytes.len()-at<width { return -1 }
    let second=i64(bytes[at+1])
    if second<128 || second>191 { return -1 }
    if (first==224 && second<160) || (first==237 && second>159) ||
       (first==240 && second<144) || (first==244 && second>143) { return -1 }
    var part:i64=2
    while part<width {
        let byte=i64(bytes[at+part])
        if byte<128 || byte>191 { return -1 }
        part+=1
    }
    return at+width
}

// Scan the ENTIRE input even if both requested boundaries are already found.
// error reports the first byte of the malformed scalar; -1 means valid UTF-8.
pub fn locate(bytes:&[u8], begin:i64, end:i64)->Bounds {
    var first:i64=-1
    var last:i64=-1
    var count:i64=0
    var at:i64=0
    while at<bytes.len() {
        if count==begin { first=at }
        if count==end { last=at }
        let following=next(bytes,at)
        if following<0 { return Bounds{first:first,last:last,count:count,error:at} }
        at=following
        count+=1
    }
    if count==begin { first=at }
    if count==end { last=at }
    return Bounds{first:first,last:last,count:count,error:-1}
}

// Explicit UTF-8 scalar cursor. status: 0=end, 1=value, 2=invalid position/input.
pub struct Scalar { pub status:i32; pub value:i32; pub next:i64 }
pub fn decode(bytes:&[u8], at:i64)->Scalar {
    if at<0 || at>bytes.len() { return Scalar{status:2,value:0,next:at} }
    if at==bytes.len() { return Scalar{status:0,value:0,next:at} }
    let end=next(bytes,at)
    if end<0 { return Scalar{status:2,value:0,next:at} }
    let width=end-at
    var value=i32(bytes[at])
    if width==2 { value=value-192 }
    else if width==3 { value=value-224 }
    else if width==4 { value=value-240 }
    var i=at+1
    while i<end { value=value*64+i32(bytes[i])-128; i+=1 }
    return Scalar{status:1,value:value,next:end}
}

// A Unicode scalar excludes surrogate code points; zero denotes invalid input.
pub fn encoded_len(value:i32)->i64 {
    if value<0 || value>1114111 || (value>=55296 && value<=57343) { return 0 }
    if value<128 { return 1 }
    if value<2048 { return 2 }
    if value<65536 { return 3 }
    return 4
}

// Returns written bytes, -1 for a non-scalar or -2 for insufficient output.
// Both failures leave every output byte unchanged.
pub fn encode(value:i32, output:&mut [u8])->i64 {
    let width=encoded_len(value)
    if width==0 { return -1 }
    if output.len()<width { return -2 }
    if width==1 { output[0]=u8(value) }
    else if width==2 {
        output[0]=u8(192+value/64)
        output[1]=u8(128+value%64)
    } else if width==3 {
        output[0]=u8(224+value/4096)
        output[1]=u8(128+(value/64)%64)
        output[2]=u8(128+value%64)
    } else {
        output[0]=u8(240+value/262144)
        output[1]=u8(128+(value/4096)%64)
        output[2]=u8(128+(value/64)%64)
        output[3]=u8(128+value%64)
    }
    return width
}
