type Big=[[[[[[[[[[[[[u8;1];2];2];2];2];2];2];2];2];2];2];2];2];
enum Sum{Empty,Data(bool,Big,i64)}
fn change(value:Sum)->Sum{
 match value{Sum.Data(flag,body,tail)=>{var changed=body;changed[0][0][0][0][0][0][0][0][0][0][0][0][0]=9u8;return Sum.Data(flag,changed,tail+1)},Sum.Empty()=>{return Sum.Empty()}}
}
fn factory()->fn(Sum)->Sum{return change}
fn main()->i32{let a0=[3u8];let a1=[a0,a0];let a2=[a1,a1];let a3=[a2,a2];let a4=[a3,a3];let a5=[a4,a4];let a6=[a5,a5];let a7=[a6,a6];let a8=[a7,a7];let a9=[a8,a8];let a10=[a9,a9];let a11=[a10,a10];let a12=[a11,a11];

 let source=Sum.Data(true,a12,17);
 let pointer=factory();
 let value=pointer(source);
 var observed=0;
 match source{Sum.Data(flag,body,tail)=>{observed=i32(body[0][0][0][0][0][0][0][0][0][0][0][0][0])},Sum.Empty()=>{observed=99}}
 match value{Sum.Data(flag,body,tail)=>{if !flag{return 1}if observed!=3{return 2}if body[0][0][0][0][0][0][0][0][0][0][0][0][0]!=9u8{return 3}return i32(tail)-18},Sum.Empty()=>{return 4}}
}
