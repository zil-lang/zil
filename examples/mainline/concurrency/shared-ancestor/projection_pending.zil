struct Item { value:i32 }
fn worker(input:&i32)->i32 { return *input }
fn main()->i32 {
    let item=Item{value:9}
    let body=()->i32=>0
    let result=thread_scope(worker,&item.value,body)
    return result.1
}
