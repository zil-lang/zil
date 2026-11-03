type Big=[[[[[[[[[[[[[u8;1];2];2];2];2];2];2];2];2];2];2];2];2];
enum Failure{Short(i32),Long(Big)}
fn supply(fail:bool,value:Big)->Failure!Big{if fail{return Failure.Long(value)}return value}
fn bounce(value:Failure!Big)->Failure!Big{return value}
fn main()->Failure!(){let a0=[3u8];let a1=[a0,a0];let a2=[a1,a1];let a3=[a2,a2];let a4=[a3,a3];let a5=[a4,a4];let a6=[a5,a5];let a7=[a6,a6];let a8=[a7,a7];let a9=[a8,a8];let a10=[a9,a9];let a11=[a10,a10];let a12=[a11,a11];

 let pointer:fn(Failure!Big)->Failure!Big=bounce;
 let value=pointer(supply(false,a12))?;
 return ()
}
