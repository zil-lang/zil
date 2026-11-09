fn increment(cell:Mutex<i32>)->(){
    var guard=mutex_lock(cell)
    let value=mutex_value(&mut guard)
    *value+=1
}
fn positive(cell:Mutex<i32>)->bool{
    var guard=mutex_lock(cell)
    let value=mutex_value(&mut guard)
    return *value>0
}
fn twice(value:i64)->i64{return value+value}
fn checked(value:i32)->(i32,bool){return (value,true)}
fn worker(cell:Mutex<i32>)->i32{
    increment(cell)
    let (value,valid)=checked(3)
    if positive(cell) && twice(i64(value))==6 && valid{return 0}
    return 1
}
fn run(cell:Mutex<i32>)->i32{
    let (created,child,parent)=thread_scope(worker,cell,()->i32=>worker(cell))
    if !created{return 90}
    var guard=mutex_lock(cell)
    let value=mutex_value(&mut guard)
    if *value==2 && child==0 && parent==0{return 0}
    return 92
}
fn entry()->MutexCreateError!i32{
    let cell=mutex_create(0)?
    return run(cell)
}
fn main()->i32{
    match entry(){.ok(value)=>{return value},.err(_)=>{return 91}}
}
