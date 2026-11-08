pub struct Value{pub number:i32}
pub fn worker<life worker_bound>(pointer:&'worker_bound Value)->&'worker_bound Value{
 return pointer
}
