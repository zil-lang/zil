import rules
import model

extern "C" fn async_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),async_callback_register());effects(ffi);};
extern "C" fn async_request(token:c_handle) contract{provides(static_callbacks(),async_callback_request_close());effects(ffi);};
extern "C" fn async_drain(token:c_handle)->i32 contract{provides(static_callbacks(),async_callback_poll_drain());effects(ffi);};
extern "C" fn registration_phase_pump()->i32 contract{effects(ffi);};
extern "C" fn async_seen(value:i32) contract{effects(ffi);};
extern "C" fn async_note(code:i32) contract{effects(ffi);};
extern "C" fn registration_phase_complete()->i32 contract{effects(ffi);};

resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{async_note(.code);}}}

fn close_once<F:FnMut(i32)->i32 + ClosedEnvironment>(r:&mut AsyncRegistration<F>)->std.task.Poll<()> contract{requires(state(r,Closing,Closed));provides(on_pending(state(r,Closing)),on_ready(state(r,Closed)));} {
    return async_registration_poll_close(r)
}

fn worker(input:&i32)->i32 {
    let first=*input;
    task_yield();
    return first+*input;
}
fn start<life a>(input:&'a i32)->TaskCreateError!Task<i32> contract{result(create)} {
    return task_create(worker,input);
}

fn main()->TaskPollState!i32 {
    let value=model.bump(36);
    let number=model.number(value);
    let selected=rules.get::<model.Number>(&number);
    let mark=Mark{code:37};
    var count=selected;
    let callback=move(event:i32)->i32=>{count+=1;return event+count+mark.code-37};
    unsafe {
        var owner=c_register_async(async_register,async_request,async_drain,callback);
        async_registration_request_close(&mut owner);
        loop {
            match close_once(&mut owner) {
                std.task.Poll.Pending()=>{async_seen(0);registration_phase_pump();continue;},
                std.task.Poll.Ready(_)=>{async_seen(17);break;}
            }
        }
        async_registration_finish_destroy_checked(give owner);
        if registration_phase_complete()!=0{return 98;}
    }
    let input=selected-16;
    match start(&input) {
        .ok(created)=>{
            var task=give created;
            match task_poll(&mut task)? {
                Poll.Pending()=>{},
                Poll.Ready(_)=>{return 91;}
            }
            match task_poll(&mut task)? {
                Poll.Pending()=>{return 92;},
                Poll.Ready(result)=>{if result!=42{return 93;}}
            }
            match task_poll_close(&mut task) {
                Poll.Pending()=>{return 94;},
                Poll.Ready(_)=>{}
            }
            task_finish_destroy(give task);
        },
        .err(_)=>{return 95;}
    }
    return 42;
}
