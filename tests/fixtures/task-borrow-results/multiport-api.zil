fn worker<life a,life b>(prefix:i32,scratch:&'a i32,pointer:&'b i32)->&'b i32{
 let local=prefix
 task_yield()
 let observed=*scratch
 return pointer
}
pub fn start<life a,life b>(scratch:&'a i32,pointer:&'b i32)->TaskCreateError!Task<&'b i32,(&'a i32,&'b i32)> contract{result(give)}{
 return task_create(worker,7,scratch,pointer)
}
pub fn relay<life a,life b>(give task:Task<&'b i32,(&'a i32,&'b i32)>)->Task<&'b i32,(&'a i32,&'b i32)> contract{result(give)}{
 return give task
}
pub fn poll<life a,life b>(task:&mut Task<&'b i32,(&'a i32,&'b i32)>)->TaskPollState!Poll<&'b i32>{
 return task_poll(task)
}
