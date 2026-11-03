// 333 source fixture; native execution pending the ABI implementation.
type LargeArgument=[[[[[[[[[[[[[[[[[[[u8;1];2];2];2];2];2];2];2];2];2];2];2];2];2];2];2];2];2];2];
fn identity(value:LargeArgument)->LargeArgument{return value}
fn main()->i32{
let a0:[u8;1]=[7u8];
let a1=[a0,a0];
let a2=[a1,a1];
let a3=[a2,a2];
let a4=[a3,a3];
let a5=[a4,a4];
let a6=[a5,a5];
let a7=[a6,a6];
let a8=[a7,a7];
let a9=[a8,a8];
let a10=[a9,a9];
let a11=[a10,a10];
let a12=[a11,a11];
let a13=[a12,a12];
let a14=[a13,a13];
let a15=[a14,a14];
let a16=[a15,a15];
let a17=[a16,a16];
let a18=[a17,a17];
let result=identity(a18);return i32(result[0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0])-7;
}
