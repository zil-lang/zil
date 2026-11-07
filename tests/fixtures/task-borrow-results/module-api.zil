fn worker<life a>(pointer:&'a i32)->&'a i32{
 let local=7
 task_yield()
 return pointer
}
pub fn start<life a>(pointer:&'a i32)->TaskCreateError!Task<&'a i32,&'a i32> contract{result(give)}{
 return task_create(worker,pointer)
}
pub fn relay<life a>(give task:Task<&'a i32,&'a i32>)->Task<&'a i32,&'a i32> contract{result(give)}{
 return give task
}
pub fn poll<life a>(task:&mut Task<&'a i32,&'a i32>)->TaskPollState!Poll<&'a i32>{
 return task_poll(task)
}
