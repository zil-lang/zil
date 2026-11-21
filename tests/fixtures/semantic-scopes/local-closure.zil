fn main()->i32{
 let bias=20
 let callback=(value:i32)=>value+bias
 return callback(22)-42
}
