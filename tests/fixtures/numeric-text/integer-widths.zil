import std.int
fn main()->i32{
 match int.parse_u8("0") {
  .err(_)=>{return 1},
  .ok(value)=>{
   if value!=0u8 {return 2}
   let decimal=int.format_u8(value)
   match int.parse_u8(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 3},
    .ok(parsed)=>{if parsed!=value{return 4}}
   }
  }
 }
 match int.parse_u8("255") {
  .err(_)=>{return 5},
  .ok(value)=>{
   if value!=255u8 {return 6}
   let decimal=int.format_u8(value)
   match int.parse_u8(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 7},
    .ok(parsed)=>{if parsed!=value{return 8}}
   }
  }
 }
 match int.parse_u8("-1"){.err(_)=>{},.ok(_)=>{return 9}}
 match int.parse_u8("256"){.err(_)=>{},.ok(_)=>{return 10}}
 match int.parse_u8("+"){.err(_)=>{},.ok(_)=>{return 11}}
 match int.parse_u8("1x"){.err(_)=>{},.ok(_)=>{return 12}}
 match int.parse_u16("0") {
  .err(_)=>{return 13},
  .ok(value)=>{
   if value!=0u16 {return 14}
   let decimal=int.format_u16(value)
   match int.parse_u16(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 15},
    .ok(parsed)=>{if parsed!=value{return 16}}
   }
  }
 }
 match int.parse_u16("65535") {
  .err(_)=>{return 17},
  .ok(value)=>{
   if value!=65535u16 {return 18}
   let decimal=int.format_u16(value)
   match int.parse_u16(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 19},
    .ok(parsed)=>{if parsed!=value{return 20}}
   }
  }
 }
 match int.parse_u16("-1"){.err(_)=>{},.ok(_)=>{return 21}}
 match int.parse_u16("65536"){.err(_)=>{},.ok(_)=>{return 22}}
 match int.parse_u16("+"){.err(_)=>{},.ok(_)=>{return 23}}
 match int.parse_u16("1x"){.err(_)=>{},.ok(_)=>{return 24}}
 match int.parse_u32("0") {
  .err(_)=>{return 25},
  .ok(value)=>{
   if value!=0u32 {return 26}
   let decimal=int.format_u32(value)
   match int.parse_u32(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 27},
    .ok(parsed)=>{if parsed!=value{return 28}}
   }
  }
 }
 match int.parse_u32("4294967295") {
  .err(_)=>{return 29},
  .ok(value)=>{
   if value!=4294967295u32 {return 30}
   let decimal=int.format_u32(value)
   match int.parse_u32(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 31},
    .ok(parsed)=>{if parsed!=value{return 32}}
   }
  }
 }
 match int.parse_u32("-1"){.err(_)=>{},.ok(_)=>{return 33}}
 match int.parse_u32("4294967296"){.err(_)=>{},.ok(_)=>{return 34}}
 match int.parse_u32("+"){.err(_)=>{},.ok(_)=>{return 35}}
 match int.parse_u32("1x"){.err(_)=>{},.ok(_)=>{return 36}}
 match int.parse_u64("0") {
  .err(_)=>{return 37},
  .ok(value)=>{
   if value!=0u64 {return 38}
   let decimal=int.format_u64(value)
   match int.parse_u64(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 39},
    .ok(parsed)=>{if parsed!=value{return 40}}
   }
  }
 }
 match int.parse_u64("18446744073709551615") {
  .err(_)=>{return 41},
  .ok(value)=>{
   if value!=18446744073709551615u64 {return 42}
   let decimal=int.format_u64(value)
   match int.parse_u64(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 43},
    .ok(parsed)=>{if parsed!=value{return 44}}
   }
  }
 }
 match int.parse_u64("-1"){.err(_)=>{},.ok(_)=>{return 45}}
 match int.parse_u64("18446744073709551616"){.err(_)=>{},.ok(_)=>{return 46}}
 match int.parse_u64("+"){.err(_)=>{},.ok(_)=>{return 47}}
 match int.parse_u64("1x"){.err(_)=>{},.ok(_)=>{return 48}}
 match int.parse_usize("0") {
  .err(_)=>{return 49},
  .ok(value)=>{
   if value!=0usize {return 50}
   let decimal=int.format_usize(value)
   match int.parse_usize(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 51},
    .ok(parsed)=>{if parsed!=value{return 52}}
   }
  }
 }
 match int.parse_usize("18446744073709551615") {
  .err(_)=>{return 53},
  .ok(value)=>{
   if value!=18446744073709551615usize {return 54}
   let decimal=int.format_usize(value)
   match int.parse_usize(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 55},
    .ok(parsed)=>{if parsed!=value{return 56}}
   }
  }
 }
 match int.parse_usize("-1"){.err(_)=>{},.ok(_)=>{return 57}}
 match int.parse_usize("18446744073709551616"){.err(_)=>{},.ok(_)=>{return 58}}
 match int.parse_usize("+"){.err(_)=>{},.ok(_)=>{return 59}}
 match int.parse_usize("1x"){.err(_)=>{},.ok(_)=>{return 60}}
 match int.parse_i8("-128") {
  .err(_)=>{return 61},
  .ok(value)=>{
   if value!=-128i8 {return 62}
   let decimal=int.format_i8(value)
   match int.parse_i8(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 63},
    .ok(parsed)=>{if parsed!=value{return 64}}
   }
  }
 }
 match int.parse_i8("127") {
  .err(_)=>{return 65},
  .ok(value)=>{
   if value!=127i8 {return 66}
   let decimal=int.format_i8(value)
   match int.parse_i8(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 67},
    .ok(parsed)=>{if parsed!=value{return 68}}
   }
  }
 }
 match int.parse_i8("-129"){.err(_)=>{},.ok(_)=>{return 69}}
 match int.parse_i8("128"){.err(_)=>{},.ok(_)=>{return 70}}
 match int.parse_i8("+"){.err(_)=>{},.ok(_)=>{return 71}}
 match int.parse_i8("1x"){.err(_)=>{},.ok(_)=>{return 72}}
 match int.parse_i16("-32768") {
  .err(_)=>{return 73},
  .ok(value)=>{
   if value!=-32768i16 {return 74}
   let decimal=int.format_i16(value)
   match int.parse_i16(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 75},
    .ok(parsed)=>{if parsed!=value{return 76}}
   }
  }
 }
 match int.parse_i16("32767") {
  .err(_)=>{return 77},
  .ok(value)=>{
   if value!=32767i16 {return 78}
   let decimal=int.format_i16(value)
   match int.parse_i16(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 79},
    .ok(parsed)=>{if parsed!=value{return 80}}
   }
  }
 }
 match int.parse_i16("-32769"){.err(_)=>{},.ok(_)=>{return 81}}
 match int.parse_i16("32768"){.err(_)=>{},.ok(_)=>{return 82}}
 match int.parse_i16("+"){.err(_)=>{},.ok(_)=>{return 83}}
 match int.parse_i16("1x"){.err(_)=>{},.ok(_)=>{return 84}}
 match int.parse_i32("-2147483648") {
  .err(_)=>{return 85},
  .ok(value)=>{
   if value!=-2147483648i32 {return 86}
   let decimal=int.format_i32(value)
   match int.parse_i32(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 87},
    .ok(parsed)=>{if parsed!=value{return 88}}
   }
  }
 }
 match int.parse_i32("2147483647") {
  .err(_)=>{return 89},
  .ok(value)=>{
   if value!=2147483647i32 {return 90}
   let decimal=int.format_i32(value)
   match int.parse_i32(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 91},
    .ok(parsed)=>{if parsed!=value{return 92}}
   }
  }
 }
 match int.parse_i32("-2147483649"){.err(_)=>{},.ok(_)=>{return 93}}
 match int.parse_i32("2147483648"){.err(_)=>{},.ok(_)=>{return 94}}
 match int.parse_i32("+"){.err(_)=>{},.ok(_)=>{return 95}}
 match int.parse_i32("1x"){.err(_)=>{},.ok(_)=>{return 96}}
 match int.parse_i64("-9223372036854775808") {
  .err(_)=>{return 97},
  .ok(value)=>{
   if value!=-9223372036854775808i64 {return 98}
   let decimal=int.format_i64(value)
   match int.parse_i64(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 99},
    .ok(parsed)=>{if parsed!=value{return 100}}
   }
  }
 }
 match int.parse_i64("9223372036854775807") {
  .err(_)=>{return 101},
  .ok(value)=>{
   if value!=9223372036854775807i64 {return 102}
   let decimal=int.format_i64(value)
   match int.parse_i64(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 103},
    .ok(parsed)=>{if parsed!=value{return 104}}
   }
  }
 }
 match int.parse_i64("-9223372036854775809"){.err(_)=>{},.ok(_)=>{return 105}}
 match int.parse_i64("9223372036854775808"){.err(_)=>{},.ok(_)=>{return 106}}
 match int.parse_i64("+"){.err(_)=>{},.ok(_)=>{return 107}}
 match int.parse_i64("1x"){.err(_)=>{},.ok(_)=>{return 108}}
 match int.parse_isize("-9223372036854775808") {
  .err(_)=>{return 109},
  .ok(value)=>{
   if value!=-9223372036854775808isize {return 110}
   let decimal=int.format_isize(value)
   match int.parse_isize(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 111},
    .ok(parsed)=>{if parsed!=value{return 112}}
   }
  }
 }
 match int.parse_isize("9223372036854775807") {
  .err(_)=>{return 113},
  .ok(value)=>{
   if value!=9223372036854775807isize {return 114}
   let decimal=int.format_isize(value)
   match int.parse_isize(&decimal.bytes[0..decimal.len]){
    .err(_)=>{return 115},
    .ok(parsed)=>{if parsed!=value{return 116}}
   }
  }
 }
 match int.parse_isize("-9223372036854775809"){.err(_)=>{},.ok(_)=>{return 117}}
 match int.parse_isize("9223372036854775808"){.err(_)=>{},.ok(_)=>{return 118}}
 match int.parse_isize("+"){.err(_)=>{},.ok(_)=>{return 119}}
 match int.parse_isize("1x"){.err(_)=>{},.ok(_)=>{return 120}}
 return 0
}
