struct Parcel<life a,T:Copy,#N:i32>{ok:bool;tiny:u8;short:u16;tag:i32;wide:i64;bits:u64;size:usize;narrow:f32;large:f64;unit:()}
fn make<T:Copy,#N:i32,life a>(marker:Type)->Parcel<'a,T,N>{return Parcel::<'a,T,N>{ok:true,tiny:17u8,short:513u16,tag:N,wide:-4294967301i64,bits:18446744073709551557u64,size:1025usize,narrow:-2.5f32,large:0.125f64,unit:()}}
fn middle<life b,#M:i32,U:Copy>(marker:Type)->Parcel<'b,U,M>{return make::<U,M,'b>(marker)}
fn forward<#N:i32,life a,T:Copy>(marker:Type)->Parcel<'a,T,N>{return middle::<'a,N,T>(marker)}
fn result<life a,life b>(x:&'a i32,y:&'b i32)->u64{
let first:Parcel<'a,i32,37>=#(forward::<37,'a,i32>(i32));
let second:Parcel<'b,bool,5>=#(forward::<5,'b,bool>(bool));
if !first.ok{return 101u64}if !second.ok{return 102u64}
if first.tiny!=17u8{return 103u64}if second.tiny!=17u8{return 104u64}
if first.short!=513u16{return 105u64}if second.short!=513u16{return 106u64}
if first.tag!=37{return 107u64}if second.tag!=5{return 108u64}
if first.wide!= -4294967301i64{return 109u64}if second.wide!= -4294967301i64{return 110u64}
if first.bits!=18446744073709551557u64{return 111u64}if second.bits!=18446744073709551557u64{return 112u64}
if first.size!=1025usize{return 113u64}if second.size!=1025usize{return 114u64}
if first.narrow!= -2.5f32{return 115u64}if second.narrow!= -2.5f32{return 116u64}
if first.large!=0.125f64{return 117u64}if second.large!=0.125f64{return 118u64}
let unit1=first.unit;let unit2=second.unit;
return first.bits+u64(first.tag)-u64(second.tag)
}
fn main()->u64{let n=0;return result(&n,&n)}
