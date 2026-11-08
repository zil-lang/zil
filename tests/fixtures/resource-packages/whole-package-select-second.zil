struct Refs<life a>{pointer:&'a i32}
package Job<life a>{manages bytes:ByteBuffer;refs:Refs<'a>}
fn make<life a>(pointer:&'a i32)->Job<'a>{
    var bytes=bytes_new()
    return Job::<'a>{bytes:give bytes,refs:Refs::<'a>{pointer:pointer}}
}
fn read<life a>(job:&Job<'a>)->i32{
    let child=&job.refs
    return *child.pointer
}
fn pick_second<life a,life b>(left:&'a Job<'b>,right:&'a Job<'b>)->&'a Job<'b>{return right}fn discard<life b>(job:Job<'b>){}fn main()->i32{var ignored=3;var value=7;var spare=0;let left=make(&ignored);let right=make(&value);let window=pick_second(&left,&right);discard(left);ignored=11;spare=11;if *window.refs.pointer==7{return 0}return 1}