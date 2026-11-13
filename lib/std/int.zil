// Locale-independent signed decimal text. No whitespace, separators or NUL.
pub enum ParseError { Empty, InvalidDigit, Overflow }
pub struct Decimal { pub bytes:[u8;20]; pub len:i64 }

pub fn parse_i64(source:&[u8])->ParseError!i64 {
    if source.len()==0 {return ParseError.Empty}
    var at:i64=0
    var negative:bool=false
    if source[0]==45u8 {negative=true;at=1}
    if source[0]==43u8 {at=1}
    if at==source.len() {return ParseError.InvalidDigit}
    var lower:i64=-9223372036854775807
    if negative {lower=-9223372036854775808}
    var value:i64=0
    while at<source.len() {
        let digit=source[at]
        if digit<48u8 || digit>57u8 {return ParseError.InvalidDigit}
        let amount=i64(i32(digit)-48)
        if value<lower/10 {return ParseError.Overflow}
        let shifted=value*10
        if shifted<lower+amount {return ParseError.Overflow}
        value=shifted-amount
        at+=1
    }
    if negative {return value}
    return -value
}

pub fn format_i64(value:i64)->Decimal {
    var output=[0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
                0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8]
    var current=value
    let negative=current<0
    if current>0 {current=-current}
    var length:i64=0
    loop {
        let digit=i32(-(current%10))
        output[length]=u8(48+digit)
        length+=1
        current=current/10
        if current==0 {break}
    }
    if negative {output[length]=45u8;length+=1}
    var left:i64=0
    var right=length-1
    while left<right {
        let prior=output[left]
        output[left]=output[right]
        output[right]=prior
        left+=1
        right-=1
    }
    return Decimal{bytes:output,len:length}
}

// Full-width unsigned decimal, with an optional leading plus; minus, whitespace
// and separators are rejected. Cutoff checks precede arithmetic at every digit.
pub fn parse_u64(source:&[u8])->ParseError!u64 {
    if source.len()==0 {return ParseError.Empty}
    var at:i64=0
    if source[0]==43u8 {at=1}
    if at==source.len() {return ParseError.InvalidDigit}
    var value:u64=0u64
    while at<source.len() {
        let digit=source[at]
        if digit<48u8 || digit>57u8 {return ParseError.InvalidDigit}
        let amount=u64(digit-48u8)
        if value>1844674407370955161u64 {return ParseError.Overflow}
        if value==1844674407370955161u64 && amount>5u64 {return ParseError.Overflow}
        value=value*10u64+amount
        at+=1
    }
    return value
}
pub fn format_u64(value:u64)->Decimal {
    var output=[0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
                0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8]
    var current=value
    var length:i64=0
    loop {
        output[length]=48u8+u8(current%10u64)
        length+=1
        current=current/10u64
        if current==0u64 {break}
    }
    var left:i64=0
    var right=length-1
    while left<right {let prior=output[left];output[left]=output[right];output[right]=prior;left+=1;right-=1}
    return Decimal{bytes:output,len:length}
}

pub fn parse_u8(source:&[u8])->ParseError!u8 {
    let value=parse_u64(source)?
    if value>255u64 {return ParseError.Overflow}
    return u8(value)
}
pub fn parse_u16(source:&[u8])->ParseError!u16 {
    let value=parse_u64(source)?
    if value>65535u64 {return ParseError.Overflow}
    return u16(value)
}
pub fn parse_usize(source:&[u8])->ParseError!usize {
    let value=parse_u64(source)?
    if value>u64(~0usize) {return ParseError.Overflow}
    return usize(value)
}
pub fn format_u8(value:u8)->Decimal {return format_u64(u64(value))}
pub fn format_u16(value:u16)->Decimal {return format_u64(u64(value))}
pub fn format_usize(value:usize)->Decimal {return format_u64(u64(value))}
pub fn parse_i32(source:&[u8])->ParseError!i32 {
    let value=parse_i64(source)?
    if value< -2147483648 || value>2147483647 {return ParseError.Overflow}
    return i32(value)
}
pub fn format_i32(value:i32)->Decimal {return format_i64(i64(value))}

pub fn parse_u32(source:&[u8])->ParseError!u32 {
    let value=parse_u64(source)?
    if value>4294967295u64 {return ParseError.Overflow}
    return u32(value)
}
pub fn parse_i8(source:&[u8])->ParseError!i8 {
    let value=parse_i64(source)?
    if value< -128 || value>127 {return ParseError.Overflow}
    return i8(value)
}
pub fn parse_i16(source:&[u8])->ParseError!i16 {
    let value=parse_i64(source)?
    if value< -32768 || value>32767 {return ParseError.Overflow}
    return i16(value)
}
pub fn parse_isize(source:&[u8])->ParseError!isize {
    let value=parse_i64(source)?
    let upper=i64((~0usize)>>1usize)
    if value< -upper-1 || value>upper {return ParseError.Overflow}
    return isize(value)
}
pub fn format_u32(value:u32)->Decimal {return format_u64(u64(value))}
pub fn format_i8(value:i8)->Decimal {return format_i64(i64(value))}
pub fn format_i16(value:i16)->Decimal {return format_i64(i64(value))}
pub fn format_isize(value:isize)->Decimal {return format_i64(i64(value))}
