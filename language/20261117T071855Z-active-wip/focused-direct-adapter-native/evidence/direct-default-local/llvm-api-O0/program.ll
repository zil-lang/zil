; ZIL verified LLVM module
; verified-fingerprint: a59a523f0652a3709ffed2742dc35c0bac32b856ae698efd454b21db75be0a0f
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { ptr, { i32 }, i1, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  call void @zil.task.drop(ptr %payload.0)
  br label %done.0
done.0:
  %flag.ptr.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  call void @zil.fn.1(ptr %payload.1)
  br label %done.1
done.1:
  ret void
}
declare void @llvm.trap()
declare ptr @calloc(i64, i64)
declare void @free(ptr)
define internal { i32, i32 } @zil.task.poll(ptr %task) {
entry:
  %frame = load ptr, ptr %task, align 1
  %valid = icmp ne ptr %frame, null
  br i1 %valid, label %invoke, label %empty
invoke:
  %entry.slot = getelementptr { ptr, ptr, ptr, ptr, i32, i32, i1, i32 }, ptr %frame, i32 0, i32 0
  %entry.fn = load ptr, ptr %entry.slot, align 1
  %answer = call { i32, i32 } %entry.fn(ptr %frame)
  ret { i32, i32 } %answer
empty:
  %invalid.tag = insertvalue { i32, i32 } zeroinitializer, i32 4, 0
  %invalid.value = insertvalue { i32, i32 } %invalid.tag, i32 0, 1
  ret { i32, i32 } %invalid.value
}
define internal void @zil.task.request_cancel(ptr %task) {
entry:
  %frame = load ptr, ptr %task, align 1
  %valid = icmp ne ptr %frame, null
  br i1 %valid, label %invoke, label %empty
invoke:
  %entry.slot = getelementptr { ptr, ptr, ptr, ptr, i32, i32, i1, i32 }, ptr %frame, i32 0, i32 1
  %entry.fn = load ptr, ptr %entry.slot, align 1
  call void %entry.fn(ptr %frame)
  ret void
empty:
  ret void
}
define internal i1 @zil.task.poll_close(ptr %task) {
entry:
  %frame = load ptr, ptr %task, align 1
  %valid = icmp ne ptr %frame, null
  br i1 %valid, label %invoke, label %empty
invoke:
  %entry.slot = getelementptr { ptr, ptr, ptr, ptr, i32, i32, i1, i32 }, ptr %frame, i32 0, i32 2
  %entry.fn = load ptr, ptr %entry.slot, align 1
  %answer = call i1 %entry.fn(ptr %frame)
  ret i1 %answer
empty:
  ret i1 true
}
define internal void @zil.task.finish_destroy(ptr %task) {
entry:
  %valid = icmp ne ptr %task, null
  br i1 %valid, label %invoke, label %empty
invoke:
  %entry.slot = getelementptr { ptr, ptr, ptr, ptr, i32, i32, i1, i32 }, ptr %task, i32 0, i32 3
  %entry.fn = load ptr, ptr %entry.slot, align 1
  call void %entry.fn(ptr %task)
  ret void
empty:
  ret void
}
define internal void @zil.task.drop(ptr %head) {
entry:
  call void @zil.task.request_cancel(ptr %head)
  %closed = call i1 @zil.task.poll_close(ptr %head)
  br i1 %closed, label %destroy, label %task.trap
destroy:
  %frame = load ptr, ptr %head, align 1
  call void @zil.task.finish_destroy(ptr %frame)
  store ptr null, ptr %head, align 1
  ret void
task.trap:
  call void @llvm.trap()
  unreachable
}
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare void @quiet_retired(i32) nonlazybind
define void @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @quiet_retired(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.3(ptr %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca ptr, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca ptr, align 8
  %enum.slot.16 = alloca { i32, [0 x i8] }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store ptr %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = getelementptr i8, ptr %local.1, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  call void @zil.task.request_cancel(ptr %v.3)
  br label %bb.1
bb.1:
  %v.5 = or i1 false, true
  br label %bb.2
bb.2:
  %v.6 = getelementptr i8, ptr %local.1, i64 0
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %tmp.0 = alloca { i32, [0 x i8] }
  store { i32, [0 x i8] } zeroinitializer, ptr %tmp.0, align 1
  %tmp.1 = call i1 @zil.task.poll_close(ptr %v.8)
  %tmp.2 = zext i1 %tmp.1 to i32
  %task.general.0.tag.ptr = getelementptr { i32, [0 x i8] }, ptr %tmp.0, i32 0, i32 0
  store i32 %tmp.2, ptr %task.general.0.tag.ptr, align 1
  %v.9 = load { i32, [0 x i8] }, ptr %tmp.0
  %v.10 = extractvalue { i32, [0 x i8] } %v.9, 0
  %v.11 = add i32 0, 0
  %v.12 = icmp eq i32 %v.10, %v.11
  br i1 %v.12, label %bb.5, label %bb.4
bb.3:
  %v.17 = load ptr, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.task.finish_destroy(ptr %v.17)
  %v.19 = add i32 0, 0
  %tmp.3 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.task.drop(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.4 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.task.drop(ptr %local.0)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.19
bb.4:
  %v.13 = extractvalue { i32, [0 x i8] } %v.9, 0
  %v.14 = add i32 0, 1
  %v.15 = icmp eq i32 %v.13, %v.14
  br i1 %v.15, label %bb.7, label %bb.6
bb.5:
  br label %bb.1
bb.6:
  br label %zil.trap
bb.7:
  %tmp.5 = extractvalue { i32, [0 x i8] } %v.9, 0
  %tmp.6 = icmp ne i32 %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.3
checked.3:
  br label %bb.3
zil.trap:
  call void @llvm.trap()
  unreachable
}
define %zil_env_0 @zil.fn.4(ptr %arg.0, { i32 } %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca ptr, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 true, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca ptr, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { i32 } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store ptr %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.1 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.1, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.2 = load ptr, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.3 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, ptr %v.2, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, { i32 } %v.3, 1
  %tmp.2 = insertvalue %zil_env_0 %tmp.1, i1 true, 2
  %v.4 = insertvalue %zil_env_0 %tmp.2, i1 true, 3
  %tmp.3 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.task.drop(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.5 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.5, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.task.drop(ptr %local.0)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret %zil_env_0 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.5({ i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i32 }, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i1, { i32, [0 x i8] }, ptr }, align 8
  %partial.live.1.0 = alloca i1, align 1
  store i1 false, ptr %partial.live.1.0, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca ptr, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca ptr, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca %zil_env_0, align 8
  store { i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %partial.live.1.0, align 1
  %tmp.0 = call ptr @zil.task.init.2()
  %tmp.1 = icmp eq ptr %tmp.0, null
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, ptr } zeroinitializer, i1 %tmp.1, 0
  %v.0 = insertvalue { i1, { i32, [0 x i8] }, ptr } %tmp.2, ptr %tmp.0, 2
  store { i1, { i32, [0 x i8] }, ptr } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %partial.live.1.0, align 1
  %tmp.3 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %local.1, i32 0, i32 0
  %tmp.4 = load i1, ptr %tmp.3, align 1
  %v.1 = zext i1 %tmp.4 to i32
  %v.2 = add i32 0, 1
  %v.3 = icmp eq i32 %v.1, %v.2
  br i1 %v.3, label %bb.2, label %bb.1
bb.1:
  %tmp.5 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %local.1, i32 0, i32 0
  %tmp.6 = load i1, ptr %tmp.5, align 1
  %v.5 = zext i1 %tmp.6 to i32
  %v.6 = add i32 0, 0
  %v.7 = icmp eq i32 %v.5, %v.6
  br i1 %v.7, label %bb.4, label %bb.3
bb.2:
  %tmp.7 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.7, label %partial.drop.0, label %partial.done.0
partial.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.8 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %local.1, i32 0, i32 0
  %tmp.9 = load i1, ptr %tmp.8, align 1
  %tmp.10 = icmp eq i1 %tmp.9, false
  br i1 %tmp.10, label %partial.leaf.path.0.0.0, label %partial.leaf.done.0.0
partial.leaf.path.0.0.0:
  %tmp.11 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %local.1, i32 0, i32 2
  %tmp.12 = load i1, ptr %partial.live.1.0, align 1
  br i1 %tmp.12, label %partial.leaf.run.0.0, label %partial.leaf.done.0.0
partial.leaf.run.0.0:
  store i1 false, ptr %partial.live.1.0, align 1
  call void @zil.task.drop(ptr %tmp.11)
  br label %partial.leaf.done.0.0
partial.leaf.done.0.0:
  br label %partial.done.0
partial.done.0:
  %v.4 = add i32 0, 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %partial.live.1.0, align 1
  %tmp.13 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.13, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.fn.1(ptr %local.0)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.4
bb.3:
  br label %zil.trap
bb.4:
  %tmp.14 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %local.1, i32 0, i32 0
  %tmp.15 = load i1, ptr %tmp.14, align 1
  %tmp.16 = icmp ne i1 %tmp.15, false
  br i1 %tmp.16, label %zil.trap, label %checked.2
checked.2:
  %tmp.17 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %local.1, i32 0, i32 2
  %v.8 = load ptr, ptr %tmp.17, align 8
  store i1 false, ptr %partial.live.1.0, align 1
  %tmp.18 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.18, label %partial.drop.3, label %partial.done.3
partial.drop.3:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.19 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %local.1, i32 0, i32 0
  %tmp.20 = load i1, ptr %tmp.19, align 1
  %tmp.21 = icmp eq i1 %tmp.20, false
  br i1 %tmp.21, label %partial.leaf.path.3.0.0, label %partial.leaf.done.3.0
partial.leaf.path.3.0.0:
  %tmp.22 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %local.1, i32 0, i32 2
  %tmp.23 = load i1, ptr %partial.live.1.0, align 1
  br i1 %tmp.23, label %partial.leaf.run.3.0, label %partial.leaf.done.3.0
partial.leaf.run.3.0:
  store i1 false, ptr %partial.live.1.0, align 1
  call void @zil.task.drop(ptr %tmp.22)
  br label %partial.leaf.done.3.0
partial.leaf.done.3.0:
  br label %partial.done.3
partial.done.3:
  store i1 false, ptr %resource.live.2, align 1
  store ptr %v.8, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %v.9 = load ptr, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store ptr %v.9, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.10 = load { i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store { i32 } %v.10, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.11 = load ptr, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.12 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  %v.13 = call %zil_env_0 @zil.fn.4(ptr %v.11, { i32 } %v.12)
  %tmp.24 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.24, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.1(ptr %local.4)
  br label %resource.done.4
resource.done.4:
  %tmp.25 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.25, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.task.drop(ptr %local.3)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.5, align 1
  store %zil_env_0 %v.13, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.14 = add i32 0, 0
  %tmp.26 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.26, label %closure.drop.6, label %closure.done.6
closure.drop.6:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.closure.drop.0(ptr %local.5)
  br label %closure.done.6
closure.done.6:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %tmp.27 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.27, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.task.drop(ptr %local.2)
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %partial.live.1.0, align 1
  %tmp.28 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.28, label %resource.drop.8, label %resource.done.8
resource.drop.8:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.fn.1(ptr %local.0)
  br label %resource.done.8
resource.done.8:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.6() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = add i32 0, 7
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.3, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.4 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.5 = call i32 @zil.fn.5({ i32 } %v.4)
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.7(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  %enum.slot.17 = alloca { i32, [0 x i8] }, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.1, align 8
  %v.2 = load ptr, ptr %local.0, align 8
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  call void @zil.task.request_cancel(ptr %v.4)
  br label %bb.1
bb.1:
  %v.6 = or i1 false, true
  br label %bb.2
bb.2:
  %v.7 = load ptr, ptr %local.0, align 8
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %tmp.0 = alloca { i32, [0 x i8] }
  store { i32, [0 x i8] } zeroinitializer, ptr %tmp.0, align 1
  %tmp.1 = call i1 @zil.task.poll_close(ptr %v.9)
  %tmp.2 = zext i1 %tmp.1 to i32
  %task.general.0.tag.ptr = getelementptr { i32, [0 x i8] }, ptr %tmp.0, i32 0, i32 0
  store i32 %tmp.2, ptr %task.general.0.tag.ptr, align 1
  %v.10 = load { i32, [0 x i8] }, ptr %tmp.0
  %v.11 = extractvalue { i32, [0 x i8] } %v.10, 0
  %v.12 = add i32 0, 0
  %v.13 = icmp eq i32 %v.11, %v.12
  br i1 %v.13, label %bb.5, label %bb.4
bb.3:
  %v.18 = load ptr, ptr %local.1, align 8
  %v.19 = load ptr, ptr %local.1, align 8
  %tmp.3 = getelementptr { i32 }, ptr %v.19, i32 0, i32 0
  %v.20 = load i32, ptr %tmp.3, align 4
  %v.21 = add i32 0, 1
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.20, i32 %v.21)
  %v.22 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  %tmp.6 = getelementptr { i32 }, ptr %v.18, i32 0, i32 0
  store i32 %v.22, ptr %tmp.6, align 4
  %v.23 = load ptr, ptr %local.1, align 8
  %tmp.7 = getelementptr { i32 }, ptr %v.23, i32 0, i32 0
  %v.24 = load i32, ptr %tmp.7, align 4
  ret i32 %v.24
bb.4:
  %v.14 = extractvalue { i32, [0 x i8] } %v.10, 0
  %v.15 = add i32 0, 1
  %v.16 = icmp eq i32 %v.14, %v.15
  br i1 %v.16, label %bb.7, label %bb.6
bb.5:
  br label %bb.1
bb.6:
  br label %zil.trap
bb.7:
  %tmp.8 = extractvalue { i32, [0 x i8] } %v.10, 0
  %tmp.9 = icmp ne i32 %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  br label %bb.3
zil.trap:
  call void @llvm.trap()
  unreachable
}
%zil_task_frame_2 = type { ptr, ptr, ptr, ptr, i32, i32, i1, i32, i32 }
define internal { i32, i32 } @zil.task.poll.2(ptr %frame) {
task.entry:
  %task.state = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 4
  %task.pc = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 5
  %task.cancel = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 6
  %task.result = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 7
  %task.slot.8 = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 8
  %task.old.state = load i32, ptr %task.state, align 1
  %task.open = icmp eq i32 %task.old.state, 0
  br i1 %task.open, label %task.check.cancel, label %task.finished
task.check.cancel:
  %task.cancelled = load i1, ptr %task.cancel, align 1
  br i1 %task.cancelled, label %task.cancelled.result, label %task.dispatch
task.dispatch:
  store i32 4, ptr %task.state, align 1
  %task.saved.pc = load i32, ptr %task.pc, align 1
  br label %task.select.0
task.select.0:
  %task.pc.is.0 = icmp eq i32 %task.saved.pc, 0
  br i1 %task.pc.is.0, label %entry, label %task.select.1
task.select.1:
  %task.pc.is.2 = icmp eq i32 %task.saved.pc, 2
  br i1 %task.pc.is.2, label %task.resume.1, label %task.trap
entry:
  br label %bb.0
bb.0:
  store i32 2, ptr %task.pc, align 1
  store i32 0, ptr %task.state, align 1
  %pending.2.tag = insertvalue { i32, i32 } zeroinitializer, i32 0, 0
  %pending.2.value = insertvalue { i32, i32 } %pending.2.tag, i32 zeroinitializer, 1
  ret { i32, i32 } %pending.2.value
task.resume.1:
  %v.1 = add i32 0, 7
  store i32 %v.1, ptr %task.slot.8, align 1
  %task.reload.0 = load i32, ptr %task.slot.8, align 1
  store i32 %task.reload.0, ptr %task.result, align 1
  store i32 1, ptr %task.state, align 1
  %task.ready.1 = load i32, ptr %task.result, align 1
  %ready.2.tag = insertvalue { i32, i32 } zeroinitializer, i32 1, 0
  %ready.2.value = insertvalue { i32, i32 } %ready.2.tag, i32 %task.ready.1, 1
  ret { i32, i32 } %ready.2.value
zil.trap:
  call void @llvm.trap()
  unreachable
task.finished:
  %task.taken = icmp eq i32 %task.old.state, 1
  br i1 %task.taken, label %task.complete.result, label %task.was.cancelled
task.was.cancelled:
  %task.was.cancelled.flag = icmp eq i32 %task.old.state, 2
  br i1 %task.was.cancelled.flag, label %task.cancelled.result, label %task.invalid.result
task.complete.result:
  %task.complete.result.tag = insertvalue { i32, i32 } zeroinitializer, i32 2, 0
  %task.complete.result.value = insertvalue { i32, i32 } %task.complete.result.tag, i32 zeroinitializer, 1
  ret { i32, i32 } %task.complete.result.value
task.cancelled.result:
  store i32 2, ptr %task.state, align 1
  %task.cancelled.result.tag = insertvalue { i32, i32 } zeroinitializer, i32 3, 0
  %task.cancelled.result.value = insertvalue { i32, i32 } %task.cancelled.result.tag, i32 zeroinitializer, 1
  ret { i32, i32 } %task.cancelled.result.value
task.invalid.result:
  %task.invalid.result.tag = insertvalue { i32, i32 } zeroinitializer, i32 4, 0
  %task.invalid.result.value = insertvalue { i32, i32 } %task.invalid.result.tag, i32 zeroinitializer, 1
  ret { i32, i32 } %task.invalid.result.value
task.trap:
  call void @llvm.trap()
  unreachable
}
define internal void @zil.task.init_into.2(ptr %frame) {
entry:
  store %zil_task_frame_2 zeroinitializer, ptr %frame, align 1
  %header.0 = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 0
  store ptr @zil.task.poll.2, ptr %header.0, align 1
  %header.1 = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 1
  store ptr @zil.task.request_cancel.2, ptr %header.1, align 1
  %header.2 = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 2
  store ptr @zil.task.poll_close.2, ptr %header.2, align 1
  %header.3 = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 3
  store ptr @zil.task.finish_into.2, ptr %header.3, align 1
  ret void
}
define internal ptr @zil.task.init.2() noinline {
entry:
  %size.ptr = getelementptr %zil_task_frame_2, ptr null, i64 1
  %size = ptrtoint ptr %size.ptr to i64
  %frame = call ptr @calloc(i64 1, i64 %size)
  %allocated = icmp ne ptr %frame, null
  br i1 %allocated, label %init, label %return
init:
  call void @zil.task.init_into.2(ptr %frame)
  %owner.destroy = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 3
  store ptr @zil.task.finish_destroy.2, ptr %owner.destroy, align 1
  br label %return
return:
  ret ptr %frame
}
define internal void @zil.task.request_cancel.2(ptr %frame) {
entry:
  %cancel.state = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 4
  %cancel.previous = load i32, ptr %cancel.state, align 1
  store i32 4, ptr %cancel.state, align 1
  %cancel = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 6
  store i1 true, ptr %cancel, align 1
  store i32 %cancel.previous, ptr %cancel.state, align 1
  ret void
}
define internal i1 @zil.task.poll_close.2(ptr %frame) {
entry:
  %state = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 4
  %current = load i32, ptr %state, align 1
  %active = icmp eq i32 %current, 4
  br i1 %active, label %busy.pending, label %close
close:
  store i32 4, ptr %state, align 1
  call void @zil.task.request_cancel.2(ptr %frame)
  store i32 3, ptr %state, align 1
  ret i1 true
pending:
  store i32 5, ptr %state, align 1
  ret i1 false
busy.pending:
  ret i1 false
}
define internal void @zil.task.finish_into.2(ptr %frame) {
entry:
  %state = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 4
  %current = load i32, ptr %state, align 1
  %closed = icmp eq i32 %current, 3
  br i1 %closed, label %done, label %task.trap
done:
  ret void
task.trap:
  call void @llvm.trap()
  unreachable
}
define internal void @zil.task.finish_destroy.2(ptr %frame) {
entry:
  %state = getelementptr %zil_task_frame_2, ptr %frame, i32 0, i32 4
  %current = load i32, ptr %state, align 1
  %closed = icmp eq i32 %current, 3
  br i1 %closed, label %destroy, label %task.trap
destroy:
  call void @free(ptr %frame)
  ret void
task.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.6()
  ret i32 %entry.result
}
