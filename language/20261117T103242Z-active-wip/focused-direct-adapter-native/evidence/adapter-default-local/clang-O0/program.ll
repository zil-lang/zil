; ZIL verified LLVM module
; verified-fingerprint: 38a2d7d8e0fc134f37c9b6dcee2f3a7dda497073523246a2b4e5c639e899e43c
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { ptr, { i32 }, i1, i1 }
define internal void @zil.sum.drop.062a305609b51635311f18da7819fde771d8b330f98b02ed70498e4dd2aad4ab(ptr %payload) {
entry:
  %drop.0 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %payload, i32 0, i32 0
  %drop.1 = load i1, ptr %drop.0, align 1
  br i1 %drop.1, label %drop.2, label %drop.3
drop.2:
  %drop.5 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %payload, i32 0, i32 1
  call void @zil.sum.drop.0dbaa9fcd181a730634cd3c447538940810cbe1192db96147c2b82ee4b4a229d(ptr %drop.5)
  br label %drop.4
drop.3:
  %drop.6 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %payload, i32 0, i32 2
  call void @zil.task.drop(ptr %drop.6)
  br label %drop.4
drop.4:
  ret void
}
define internal void @zil.sum.drop.0dbaa9fcd181a730634cd3c447538940810cbe1192db96147c2b82ee4b4a229d(ptr %payload) {
entry:
  %drop.0 = getelementptr { { i32, [0 x i8] }, { { i32 } } }, ptr %payload, i32 0, i32 1
  call void @zil.sum.drop.bca3dc1e76d99bc2302b4d28aaf4e36b7d337eaea0bf8e588a872de88ed8baea(ptr %drop.0)
  ret void
}
define internal void @zil.sum.drop.bca3dc1e76d99bc2302b4d28aaf4e36b7d337eaea0bf8e588a872de88ed8baea(ptr %payload) {
entry:
  %drop.0 = getelementptr { { i32 } }, ptr %payload, i32 0, i32 0
  call void @zil.fn.1(ptr %drop.0)
  ret void
}
define internal void @zil.sum.drop.b2261691b0f3e538a5bbbb6b0b3c209de8659379ad87d4232e969b81fcd3d942(ptr %payload) {
entry:
  %drop.0 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %payload, i32 0, i32 0
  %drop.1 = load i1, ptr %drop.0, align 1
  br i1 %drop.1, label %drop.2, label %drop.3
drop.2:
  %drop.5 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %payload, i32 0, i32 1
  br label %drop.4
drop.3:
  %drop.6 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %payload, i32 0, i32 2
  call void @zil.task.drop(ptr %drop.6)
  br label %drop.4
drop.4:
  ret void
}
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
define internal i1 @zil.task.drop.poll(ptr %head) {
entry:
  call void @zil.task.request_cancel(ptr %head)
  %closed = call i1 @zil.task.poll_close(ptr %head)
  br i1 %closed, label %destroy, label %pending
destroy:
  %frame = load ptr, ptr %head, align 1
  call void @zil.task.finish_destroy(ptr %frame)
  store ptr null, ptr %head, align 1
  ret i1 true
pending:
  ret i1 false
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
define i32 @zil.fn.6() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, align 8
  %partial.live.3.0 = alloca i1, align 1
  store i1 false, ptr %partial.live.3.0, align 1
  %partial.live.3.1 = alloca i1, align 1
  store i1 false, ptr %partial.live.3.1, align 1
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { i32, [0 x i8] }, { { i32 } } }, align 4
  %package.live.4.1 = alloca i1, align 1
  store i1 false, ptr %package.live.4.1, align 1
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { { i32 } }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca ptr, align 8
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca ptr, align 8
  %local.9 = alloca i32, align 4
  %local.10 = alloca { i32, [1 x i32] }, align 4
  %local.11 = alloca i32, align 4
  %resource.live.12 = alloca i1, align 1
  store i1 false, ptr %resource.live.12, align 1
  %local.12 = alloca ptr, align 8
  %enum.slot.39 = alloca { i32, [1 x i32] }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %partial.live.3.0, align 1
  store i1 false, ptr %partial.live.3.1, align 1
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
  %tmp.2 = alloca { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }
  store { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr } zeroinitializer, ptr %tmp.2, align 1
  %tmp.0 = call ptr @zil.task.init_owned.5({ i32 } %v.4)
  %tmp.1 = icmp eq ptr %tmp.0, null
  br i1 %tmp.1, label %task.create.owned.0.failure, label %task.create.owned.0.created
task.create.owned.0.failure:
  %tmp.3 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %tmp.2, i32 0, i32 0
  store i1 true, ptr %tmp.3, align 1
  %tmp.4 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %tmp.2, i32 0, i32 1
  %tmp.5 = getelementptr { { i32, [0 x i8] }, { { i32 } } }, ptr %tmp.4, i32 0, i32 1
  %tmp.6 = insertvalue { { i32 } } zeroinitializer, { i32 } %v.4, 0
  store { { i32 } } %tmp.6, ptr %tmp.5, align 1
  br label %task.create.owned.0.done
task.create.owned.0.created:
  %tmp.7 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %tmp.2, i32 0, i32 2
  store ptr %tmp.0, ptr %tmp.7, align 1
  br label %task.create.owned.0.done
task.create.owned.0.done:
  %v.5 = load { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %tmp.2, align 1
  %tmp.8 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.8, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr } %v.5, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  store i1 true, ptr %partial.live.3.0, align 1
  store i1 true, ptr %partial.live.3.1, align 1
  %tmp.9 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 0
  %tmp.10 = load i1, ptr %tmp.9, align 1
  %v.6 = zext i1 %tmp.10 to i32
  %v.7 = add i32 0, 1
  %v.8 = icmp eq i32 %v.6, %v.7
  br i1 %v.8, label %bb.2, label %bb.1
bb.1:
  %tmp.11 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 0
  %tmp.12 = load i1, ptr %tmp.11, align 1
  %v.12 = zext i1 %tmp.12 to i32
  %v.13 = add i32 0, 0
  %v.14 = icmp eq i32 %v.12, %v.13
  br i1 %v.14, label %bb.4, label %bb.3
bb.2:
  %tmp.13 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 0
  %tmp.14 = load i1, ptr %tmp.13, align 1
  %tmp.15 = icmp ne i1 %tmp.14, true
  br i1 %tmp.15, label %zil.trap, label %checked.2
checked.2:
  %tmp.16 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 1
  %v.9 = load { { i32, [0 x i8] }, { { i32 } } }, ptr %tmp.16, align 4
  store i1 false, ptr %partial.live.3.0, align 1
  %tmp.17 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.17, label %partial.drop.3, label %partial.done.3
partial.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.18 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 0
  %tmp.19 = load i1, ptr %tmp.18, align 1
  %tmp.20 = icmp eq i1 %tmp.19, true
  br i1 %tmp.20, label %partial.leaf.path.3.0.0, label %partial.leaf.done.3.0
partial.leaf.path.3.0.0:
  %tmp.21 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 1
  %tmp.22 = getelementptr { { i32, [0 x i8] }, { { i32 } } }, ptr %tmp.21, i32 0, i32 1
  %tmp.23 = getelementptr { { i32 } }, ptr %tmp.22, i32 0, i32 0
  %tmp.24 = load i1, ptr %partial.live.3.0, align 1
  br i1 %tmp.24, label %partial.leaf.run.3.0, label %partial.leaf.done.3.0
partial.leaf.run.3.0:
  store i1 false, ptr %partial.live.3.0, align 1
  call void @zil.fn.1(ptr %tmp.23)
  br label %partial.leaf.done.3.0
partial.leaf.done.3.0:
  %tmp.25 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 0
  %tmp.26 = load i1, ptr %tmp.25, align 1
  %tmp.27 = icmp eq i1 %tmp.26, false
  br i1 %tmp.27, label %partial.leaf.path.3.1.0, label %partial.leaf.done.3.1
partial.leaf.path.3.1.0:
  %tmp.28 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 2
  %tmp.29 = load i1, ptr %partial.live.3.1, align 1
  br i1 %tmp.29, label %partial.leaf.run.3.1, label %partial.leaf.done.3.1
partial.leaf.run.3.1:
  store i1 false, ptr %partial.live.3.1, align 1
  call void @zil.task.drop(ptr %tmp.28)
  br label %partial.leaf.done.3.1
partial.leaf.done.3.1:
  br label %partial.done.3
partial.done.3:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %package.live.4.1, align 1
  store { { i32, [0 x i8] }, { { i32 } } } %v.9, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  store i1 true, ptr %package.live.4.1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.30 = getelementptr { { i32, [0 x i8] }, { { i32 } } }, ptr %local.4, i32 0, i32 1
  %v.10 = load { { i32 } }, ptr %tmp.30, align 4
  store i1 false, ptr %package.live.4.1, align 1
  store { { i32 } } %v.10, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.6, align 1
  %tmp.31 = getelementptr { { i32 } }, ptr %local.5, i32 0, i32 0
  %tmp.32 = load { i32 }, ptr %tmp.31, align 4
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %tmp.32, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %tmp.33 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.33, label %sum.drop.4, label %sum.done.4
sum.drop.4:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.sum.drop.bca3dc1e76d99bc2302b4d28aaf4e36b7d337eaea0bf8e588a872de88ed8baea(ptr %local.5)
  br label %sum.done.4
sum.done.4:
  %v.11 = add i32 0, 1
  %tmp.34 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.34, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.1(ptr %local.6)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.35 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.35, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.36 = load i1, ptr %package.live.4.1, align 1
  br i1 %tmp.36, label %package.field.drop.7, label %package.field.done.7
package.field.drop.7:
  store i1 false, ptr %package.live.4.1, align 1
  %tmp.37 = getelementptr { { i32, [0 x i8] }, { { i32 } } }, ptr %local.4, i32 0, i32 1
  call void @zil.sum.drop.bca3dc1e76d99bc2302b4d28aaf4e36b7d337eaea0bf8e588a872de88ed8baea(ptr %tmp.37)
  br label %package.field.done.7
package.field.done.7:
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %package.live.4.1, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %partial.live.3.0, align 1
  store i1 false, ptr %partial.live.3.1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.38 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.38, label %resource.drop.8, label %resource.done.8
resource.drop.8:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.8
resource.done.8:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.11
bb.3:
  br label %zil.trap
bb.4:
  %tmp.39 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 0
  %tmp.40 = load i1, ptr %tmp.39, align 1
  %tmp.41 = icmp ne i1 %tmp.40, false
  br i1 %tmp.41, label %zil.trap, label %checked.9
checked.9:
  %tmp.42 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 2
  %v.15 = load ptr, ptr %tmp.42, align 8
  store i1 false, ptr %partial.live.3.1, align 1
  %tmp.43 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.43, label %partial.drop.10, label %partial.done.10
partial.drop.10:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.44 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 0
  %tmp.45 = load i1, ptr %tmp.44, align 1
  %tmp.46 = icmp eq i1 %tmp.45, true
  br i1 %tmp.46, label %partial.leaf.path.10.0.0, label %partial.leaf.done.10.0
partial.leaf.path.10.0.0:
  %tmp.47 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 1
  %tmp.48 = getelementptr { { i32, [0 x i8] }, { { i32 } } }, ptr %tmp.47, i32 0, i32 1
  %tmp.49 = getelementptr { { i32 } }, ptr %tmp.48, i32 0, i32 0
  %tmp.50 = load i1, ptr %partial.live.3.0, align 1
  br i1 %tmp.50, label %partial.leaf.run.10.0, label %partial.leaf.done.10.0
partial.leaf.run.10.0:
  store i1 false, ptr %partial.live.3.0, align 1
  call void @zil.fn.1(ptr %tmp.49)
  br label %partial.leaf.done.10.0
partial.leaf.done.10.0:
  %tmp.51 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 0
  %tmp.52 = load i1, ptr %tmp.51, align 1
  %tmp.53 = icmp eq i1 %tmp.52, false
  br i1 %tmp.53, label %partial.leaf.path.10.1.0, label %partial.leaf.done.10.1
partial.leaf.path.10.1.0:
  %tmp.54 = getelementptr { i1, { { i32, [0 x i8] }, { { i32 } } }, ptr }, ptr %local.3, i32 0, i32 2
  %tmp.55 = load i1, ptr %partial.live.3.1, align 1
  br i1 %tmp.55, label %partial.leaf.run.10.1, label %partial.leaf.done.10.1
partial.leaf.run.10.1:
  store i1 false, ptr %partial.live.3.1, align 1
  call void @zil.task.drop(ptr %tmp.54)
  br label %partial.leaf.done.10.1
partial.leaf.done.10.1:
  br label %partial.done.10
partial.done.10:
  store i1 false, ptr %resource.live.7, align 1
  store ptr %v.15, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.12, align 1
  %v.16 = load ptr, ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.8, align 1
  store ptr %v.16, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  %v.17 = add i32 0, 91
  store i32 %v.17, ptr %local.9, align 4
  br label %bb.5
bb.5:
  %v.18 = or i1 false, true
  br label %bb.6
bb.6:
  %v.19 = getelementptr i8, ptr %local.8, i64 0
  %v.20 = getelementptr i8, ptr %v.19, i64 0
  %v.21 = getelementptr i8, ptr %v.20, i64 0
  %tmp.56 = alloca { i1, { i32, [0 x i8] }, { i32, [1 x i32] } }
  store { i1, { i32, [0 x i8] }, { i32, [1 x i32] } } zeroinitializer, ptr %tmp.56, align 1
  %tmp.57 = call { i32, i32 } @zil.task.poll(ptr %v.21)
  %tmp.58 = extractvalue { i32, i32 } %tmp.57, 0
  %task.general.11.ready.test = icmp eq i32 %tmp.58, 1
  br i1 %task.general.11.ready.test, label %task.general.11.ready, label %task.general.11.not.ready
task.general.11.ready:
  %task.general.11.poll.ptr = getelementptr { i1, { i32, [0 x i8] }, { i32, [1 x i32] } }, ptr %tmp.56, i32 0, i32 2
  %task.general.11.poll.tag.ptr = getelementptr { i32, [1 x i32] }, ptr %task.general.11.poll.ptr, i32 0, i32 0
  store i32 1, ptr %task.general.11.poll.tag.ptr, align 1
  %task.general.11.payload.ptr = getelementptr { i32, [1 x i32] }, ptr %task.general.11.poll.ptr, i32 0, i32 1
  %task.general.11.payload = extractvalue { i32, i32 } %tmp.57, 1
  store i32 %task.general.11.payload, ptr %task.general.11.payload.ptr, align 1
  br label %task.general.11.done
task.general.11.not.ready:
  %task.general.11.pending.test = icmp eq i32 %tmp.58, 0
  br i1 %task.general.11.pending.test, label %task.general.11.done, label %task.general.11.error
task.general.11.error:
  %task.general.11.error.flag = getelementptr { i1, { i32, [0 x i8] }, { i32, [1 x i32] } }, ptr %tmp.56, i32 0, i32 0
  store i1 true, ptr %task.general.11.error.flag, align 1
  %task.general.11.error.ptr = getelementptr { i1, { i32, [0 x i8] }, { i32, [1 x i32] } }, ptr %tmp.56, i32 0, i32 1
  %task.general.11.error.tag.ptr = getelementptr { i32, [0 x i8] }, ptr %task.general.11.error.ptr, i32 0, i32 0
  %task.general.11.error.tag = sub i32 %tmp.58, 2
  store i32 %task.general.11.error.tag, ptr %task.general.11.error.tag.ptr, align 1
  br label %task.general.11.done
task.general.11.done:
  %v.22 = load { i1, { i32, [0 x i8] }, { i32, [1 x i32] } }, ptr %tmp.56
  %v.23 = extractvalue { i1, { i32, [0 x i8] }, { i32, [1 x i32] } } %v.22, 0
  %v.24 = or i1 false, true
  %v.25 = icmp eq i1 %v.23, %v.24
  br i1 %v.25, label %bb.9, label %bb.8
bb.7:
  %v.41 = load ptr, ptr %local.8, align 8
  store i1 false, ptr %resource.live.8, align 1
  store ptr %v.41, ptr %local.12, align 8
  store i1 true, ptr %resource.live.12, align 1
  %v.42 = load ptr, ptr %local.12, align 8
  store i1 false, ptr %resource.live.12, align 1
  %v.43 = call i32 @zil.fn.3(ptr %v.42)
  %tmp.59 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.59, label %resource.drop.12, label %resource.done.12
resource.drop.12:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.task.drop(ptr %local.12)
  br label %resource.done.12
resource.done.12:
  %v.44 = load i32, ptr %local.9, align 4
  store i1 false, ptr %resource.live.12, align 1
  %tmp.60 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.60, label %resource.drop.13, label %resource.done.13
resource.drop.13:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.task.drop(ptr %local.8)
  br label %resource.done.13
resource.done.13:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.61 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.61, label %resource.drop.14, label %resource.done.14
resource.drop.14:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.task.drop(ptr %local.7)
  br label %resource.done.14
resource.done.14:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %partial.live.3.0, align 1
  store i1 false, ptr %partial.live.3.1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.62 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.62, label %resource.drop.15, label %resource.done.15
resource.drop.15:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.15
resource.done.15:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.44
bb.8:
  %v.28 = extractvalue { i1, { i32, [0 x i8] }, { i32, [1 x i32] } } %v.22, 0
  %v.29 = or i1 false, false
  %v.30 = icmp eq i1 %v.28, %v.29
  br i1 %v.30, label %bb.11, label %bb.10
bb.9:
  %tmp.63 = extractvalue { i1, { i32, [0 x i8] }, { i32, [1 x i32] } } %v.22, 0
  %tmp.64 = icmp ne i1 %tmp.63, true
  br i1 %tmp.64, label %zil.trap, label %checked.16
checked.16:
  %v.26 = extractvalue { i1, { i32, [0 x i8] }, { i32, [1 x i32] } } %v.22, 1
  %v.27 = add i32 0, 92
  store i32 %v.27, ptr %local.9, align 4
  br label %bb.7
bb.10:
  br label %zil.trap
bb.11:
  %tmp.65 = extractvalue { i1, { i32, [0 x i8] }, { i32, [1 x i32] } } %v.22, 0
  %tmp.66 = icmp ne i1 %tmp.65, false
  br i1 %tmp.66, label %zil.trap, label %checked.17
checked.17:
  %v.31 = extractvalue { i1, { i32, [0 x i8] }, { i32, [1 x i32] } } %v.22, 2
  store { i32, [1 x i32] } %v.31, ptr %local.10, align 4
  %v.32 = load { i32, [1 x i32] }, ptr %local.10, align 4
  %v.33 = extractvalue { i32, [1 x i32] } %v.32, 0
  %v.34 = add i32 0, 0
  %v.35 = icmp eq i32 %v.33, %v.34
  br i1 %v.35, label %bb.13, label %bb.12
bb.12:
  %v.36 = extractvalue { i32, [1 x i32] } %v.32, 0
  %v.37 = add i32 0, 1
  %v.38 = icmp eq i32 %v.36, %v.37
  br i1 %v.38, label %bb.15, label %bb.14
bb.13:
  br label %bb.5
bb.14:
  br label %zil.trap
bb.15:
  %tmp.67 = extractvalue { i32, [1 x i32] } %v.32, 0
  %tmp.68 = icmp ne i32 %tmp.67, 1
  br i1 %tmp.68, label %zil.trap, label %checked.18
checked.18:
  store { i32, [1 x i32] } %v.32, ptr %enum.slot.39, align 4
  %tmp.69 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.39, i32 0, i32 1
  %tmp.70 = getelementptr { i32 }, ptr %tmp.69, i32 0, i32 0
  %v.39 = load i32, ptr %tmp.70, align 4
  store i32 %v.39, ptr %local.11, align 4
  %v.40 = load i32, ptr %local.11, align 4
  store i32 %v.40, ptr %local.9, align 4
  br label %bb.7
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
define internal void @zil.task.owner.unlink(ptr %node) {
entry:
  %next.1 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %node, i32 0, i32 1
  %next.value = load ptr, ptr %next.1, align 1
  %live = icmp ne ptr %next.value, null
  br i1 %live, label %unlink, label %done
unlink:
  %prev.0 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %node, i32 0, i32 0
  %prev.value = load ptr, ptr %prev.0, align 1
  %prev.next.1 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %prev.value, i32 0, i32 1
  store ptr %next.value, ptr %prev.next.1, align 1
  %next.prev.0 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %next.value, i32 0, i32 0
  store ptr %prev.value, ptr %next.prev.0, align 1
  %node.prev.0 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %node, i32 0, i32 0
  store ptr null, ptr %node.prev.0, align 1
  %node.next.1 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %node, i32 0, i32 1
  store ptr null, ptr %node.next.1, align 1
  br label %done
done:
  ret void
}
define internal void @zil.task.owner.link(ptr %node, ptr %anchor) {
entry:
  call void @zil.task.owner.unlink(ptr %node)
  %tail.0 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %anchor, i32 0, i32 0
  %tail.value = load ptr, ptr %tail.0, align 1
  %node.prev.0 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %node, i32 0, i32 0
  store ptr %tail.value, ptr %node.prev.0, align 1
  %node.next.1 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %node, i32 0, i32 1
  store ptr %anchor, ptr %node.next.1, align 1
  %tail.next.1 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %tail.value, i32 0, i32 1
  store ptr %node, ptr %tail.next.1, align 1
  %anchor.prev.0 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %anchor, i32 0, i32 0
  store ptr %node, ptr %anchor.prev.0, align 1
  ret void
}
define internal void @zil.task.owner.insert_after(ptr %node, ptr %after) {
entry:
  call void @zil.task.owner.unlink(ptr %node)
  %successor.1 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %after, i32 0, i32 1
  %successor.value = load ptr, ptr %successor.1, align 1
  %node.prev.0 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %node, i32 0, i32 0
  store ptr %after, ptr %node.prev.0, align 1
  %node.next.1 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %node, i32 0, i32 1
  store ptr %successor.value, ptr %node.next.1, align 1
  %after.next.1 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %after, i32 0, i32 1
  store ptr %node, ptr %after.next.1, align 1
  %next.prev.0 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %successor.value, i32 0, i32 0
  store ptr %node, ptr %next.prev.0, align 1
  ret void
}
define internal i1 @zil.task.owner.drain(ptr %anchor) {
entry:
  br label %loop
loop:
  %tail.0 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %anchor, i32 0, i32 0
  %tail.value = load ptr, ptr %tail.0, align 1
  %empty = icmp eq ptr %tail.value, %anchor
  br i1 %empty, label %done, label %check
check:
  %poll.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %tail.value, i32 0, i32 4
  %poll.value = load ptr, ptr %poll.4, align 1
  %payload.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %tail.value, i32 0, i32 3
  %payload.value = load ptr, ptr %payload.3, align 1
  %async = icmp ne ptr %poll.value, null
  br i1 %async, label %poll, label %drop
poll:
  %ready = call i1 %poll.value(ptr %payload.value)
  br i1 %ready, label %drop, label %pending
drop:
  call void @zil.task.owner.unlink(ptr %tail.value)
  %drop.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %tail.value, i32 0, i32 2
  %drop.value = load ptr, ptr %drop.2, align 1
  %has.drop = icmp ne ptr %drop.value, null
  br i1 %has.drop, label %invoke, label %loop
invoke:
  call void %drop.value(ptr %payload.value)
  br label %loop
pending:
  ret i1 false
done:
  ret i1 true
}
define internal void @zil.task.owner.request(ptr %anchor) {
entry:
  %first.1 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %anchor, i32 0, i32 1
  %first.value = load ptr, ptr %first.1, align 1
  br label %loop
loop:
  %node = phi ptr [ %first.value, %entry ], [ %next.value, %advance ]
  %empty = icmp eq ptr %node, %anchor
  br i1 %empty, label %done, label %check
check:
  %request.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %node, i32 0, i32 5
  %request.value = load ptr, ptr %request.5, align 1
  %payload.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %node, i32 0, i32 3
  %payload.value = load ptr, ptr %payload.3, align 1
  %has.request = icmp ne ptr %request.value, null
  br i1 %has.request, label %invoke, label %advance
invoke:
  call void %request.value(ptr %payload.value)
  br label %advance
advance:
  %next.1 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %node, i32 0, i32 1
  %next.value = load ptr, ptr %next.1, align 1
  br label %loop
done:
  ret void
}
define internal i1 @zil.task.group.0.poll(ptr %env) {
entry:
  br label %check.0
check.0:
  %flag.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %live.0 = load i1, ptr %flag.0, align 1
  br i1 %live.0, label %leaf.0, label %check.1
leaf.0:
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %frame.0 = load ptr, ptr %payload.0, align 1
  %empty.0 = icmp eq ptr %frame.0, null
  br i1 %empty.0, label %finish.0, label %active.0
active.0:
  call void @zil.task.request_cancel(ptr %payload.0)
  %ready.0 = call i1 @zil.task.poll_close(ptr %payload.0)
  br i1 %ready.0, label %destroy.0, label %pending
destroy.0:
  call void @zil.task.finish_destroy(ptr %frame.0)
  store ptr null, ptr %payload.0, align 1
  br label %finish.0
finish.0:
  store i1 false, ptr %flag.0, align 1
  br label %check.1
check.1:
  %flag.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.1, align 1
  br i1 %live.1, label %leaf.1, label %check.2
leaf.1:
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  call void @zil.fn.1(ptr %payload.1)
  br label %finish.1
finish.1:
  store i1 false, ptr %flag.1, align 1
  br label %check.2
check.2:
  ret i1 true
pending:
  ret i1 false
}
define internal void @zil.task.group.0.request(ptr %env) {
entry:
  br label %check.0
check.0:
  %flag.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %live.0 = load i1, ptr %flag.0, align 1
  br i1 %live.0, label %leaf.0, label %check.1
leaf.0:
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  call void @zil.task.request_cancel(ptr %payload.0)
  br label %check.1
check.1:
  %flag.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.1, align 1
  br i1 %live.1, label %leaf.1, label %check.2
leaf.1:
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  br label %check.2
check.2:
  ret void
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
%zil_task_frame_5 = type { ptr, ptr, ptr, ptr, i32, i32, i1, i32, { i32 }, i1, { i32 }, i1, { i1, { i32, [0 x i8] }, ptr }, i1, i1, ptr, i1, ptr, i1, { i32 }, i1, %zil_env_0, ptr, i1, { i1, { i32, [0 x i8] }, ptr }, { i1, { i32, [0 x i8] }, ptr }, ptr, i1, i32, i32, i1, ptr, i1, i32, i32, i1, i1, ptr, i1, i1, ptr, i1, i32, i1, ptr, i1, i1, ptr, ptr, i1, ptr, i1, i1, ptr, i1, ptr, { i32 }, ptr, { i32 }, %zil_env_0, i1, i1, i32, i1, i1, i1, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr }, { ptr, ptr, ptr, ptr, ptr, ptr } }
define internal { i32, i32 } @zil.task.poll.5(ptr %frame) {
task.entry:
  %task.owners.anchor = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 66
  %task.owner.0 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 67
  %task.owner.1 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 68
  %task.owner.2 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 69
  %task.owner.3 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 70
  %task.owner.4 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 71
  %task.owner.5 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 72
  %task.owner.6 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 73
  %task.owner.7 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 74
  %task.owner.8 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 75
  %task.owner.9 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 76
  %task.owner.10 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 77
  %task.owner.11 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 78
  %task.owner.12 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 79
  %task.state = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 4
  %task.pc = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 5
  %task.cancel = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 6
  %task.result = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 7
  %task.slot.8 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 8
  %task.slot.10 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 10
  %task.slot.12 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 12
  %task.slot.15 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 15
  %task.slot.17 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 17
  %task.slot.19 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 19
  %task.slot.21 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 21
  %task.slot.13 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 13
  %task.slot.9 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 9
  %task.slot.11 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 11
  %task.slot.14 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 14
  %task.slot.16 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 16
  %task.slot.18 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 18
  %task.slot.20 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 20
  %task.slot.22 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 22
  %task.slot.23 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 23
  %task.slot.39 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 39
  %task.slot.40 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 40
  %task.slot.41 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 41
  %task.slot.43 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 43
  %task.slot.44 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 44
  %task.slot.45 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 45
  %task.slot.46 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 46
  %task.slot.47 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 47
  %task.slot.49 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 49
  %task.slot.50 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 50
  %task.slot.24 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 24
  %task.slot.51 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 51
  %task.slot.52 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 52
  %task.slot.53 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 53
  %task.slot.54 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 54
  %task.slot.60 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 60
  %task.slot.61 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 61
  %task.slot.63 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 63
  %task.slot.64 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 64
  %task.slot.65 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 65
  %task.slot.26 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 26
  %task.slot.27 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 27
  %task.slot.31 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 31
  %task.slot.32 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 32
  %task.slot.36 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 36
  %task.slot.37 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 37
  %task.slot.38 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 38
  %task.slot.25 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 25
  %task.slot.28 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 28
  %task.slot.56 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 56
  %task.slot.57 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 57
  %task.slot.58 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 58
  %task.slot.59 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 59
  %task.slot.62 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 62
  %task.slot.29 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 29
  %task.slot.30 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 30
  %task.slot.42 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 42
  %task.slot.33 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 33
  %task.slot.34 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 34
  %task.slot.35 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 35
  %task.slot.48 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 48
  %task.slot.55 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 55
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
  %task.pc.is.22 = icmp eq i32 %task.saved.pc, 22
  br i1 %task.pc.is.22, label %async.drop.21.poll, label %task.select.2
task.select.2:
  %task.pc.is.27 = icmp eq i32 %task.saved.pc, 27
  br i1 %task.pc.is.27, label %async.drop.26.poll, label %task.select.3
task.select.3:
  %task.pc.is.32 = icmp eq i32 %task.saved.pc, 32
  br i1 %task.pc.is.32, label %async.drop.31.poll, label %task.trap
entry:
  store i1 true, ptr %task.slot.9, align 1
  store i1 false, ptr %task.slot.11, align 1
  store i1 false, ptr %task.slot.13, align 1
  store i1 false, ptr %task.slot.14, align 1
  store i1 false, ptr %task.slot.16, align 1
  store i1 false, ptr %task.slot.18, align 1
  store i1 false, ptr %task.slot.20, align 1
  %task.reload.0 = load { i32 }, ptr %task.slot.8, align 1
  store { i32 } %task.reload.0, ptr %task.slot.10, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %task.slot.11, align 1
  store i1 false, ptr %task.slot.13, align 1
  %tmp.0 = call ptr @zil.task.init.2()
  store ptr %tmp.0, ptr %task.slot.22, align 1
  %task.reload.1 = load ptr, ptr %task.slot.22, align 1
  %tmp.1 = icmp eq ptr %task.reload.1, null
  store i1 %tmp.1, ptr %task.slot.23, align 1
  %task.reload.2 = load i1, ptr %task.slot.23, align 1
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, ptr } zeroinitializer, i1 %task.reload.2, 0
  store { i1, { i32, [0 x i8] }, ptr } %tmp.2, ptr %task.slot.24, align 1
  %task.reload.3 = load { i1, { i32, [0 x i8] }, ptr }, ptr %task.slot.24, align 1
  %task.reload.4 = load ptr, ptr %task.slot.22, align 1
  %v.0 = insertvalue { i1, { i32, [0 x i8] }, ptr } %task.reload.3, ptr %task.reload.4, 2
  store { i1, { i32, [0 x i8] }, ptr } %v.0, ptr %task.slot.25, align 1
  call void @zil.task.owner.link(ptr %task.owner.6, ptr %task.owners.anchor)
  call void @zil.task.owner.unlink(ptr %task.owner.6)
  %task.reload.5 = load { i1, { i32, [0 x i8] }, ptr }, ptr %task.slot.25, align 1
  store { i1, { i32, [0 x i8] }, ptr } %task.reload.5, ptr %task.slot.12, align 8
  store i1 true, ptr %task.slot.11, align 1
  store i1 true, ptr %task.slot.13, align 1
  call void @zil.task.owner.link(ptr %task.owner.1, ptr %task.owners.anchor)
  %tmp.3 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %task.slot.12, i32 0, i32 0
  store ptr %tmp.3, ptr %task.slot.26, align 1
  %task.reload.6 = load ptr, ptr %task.slot.26, align 1
  %tmp.4 = load i1, ptr %task.reload.6, align 1
  store i1 %tmp.4, ptr %task.slot.27, align 1
  %task.reload.7 = load i1, ptr %task.slot.27, align 1
  %v.1 = zext i1 %task.reload.7 to i32
  store i32 %v.1, ptr %task.slot.28, align 1
  %v.2 = add i32 0, 1
  store i32 %v.2, ptr %task.slot.29, align 1
  %task.reload.8 = load i32, ptr %task.slot.28, align 1
  %task.reload.9 = load i32, ptr %task.slot.29, align 1
  %v.3 = icmp eq i32 %task.reload.8, %task.reload.9
  store i1 %v.3, ptr %task.slot.30, align 1
  %task.reload.10 = load i1, ptr %task.slot.30, align 1
  br i1 %task.reload.10, label %bb.2, label %bb.1
bb.1:
  %tmp.5 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %task.slot.12, i32 0, i32 0
  store ptr %tmp.5, ptr %task.slot.31, align 1
  %task.reload.11 = load ptr, ptr %task.slot.31, align 1
  %tmp.6 = load i1, ptr %task.reload.11, align 1
  store i1 %tmp.6, ptr %task.slot.32, align 1
  %task.reload.12 = load i1, ptr %task.slot.32, align 1
  %v.5 = zext i1 %task.reload.12 to i32
  store i32 %v.5, ptr %task.slot.33, align 1
  %v.6 = add i32 0, 0
  store i32 %v.6, ptr %task.slot.34, align 1
  %task.reload.13 = load i32, ptr %task.slot.33, align 1
  %task.reload.14 = load i32, ptr %task.slot.34, align 1
  %v.7 = icmp eq i32 %task.reload.13, %task.reload.14
  store i1 %v.7, ptr %task.slot.35, align 1
  %task.reload.15 = load i1, ptr %task.slot.35, align 1
  br i1 %task.reload.15, label %bb.4, label %bb.3
bb.2:
  call void @zil.task.owner.unlink(ptr %task.owner.1)
  %tmp.7 = load i1, ptr %task.slot.11, align 1
  store i1 %tmp.7, ptr %task.slot.36, align 1
  %task.reload.16 = load i1, ptr %task.slot.36, align 1
  br i1 %task.reload.16, label %partial.drop.0, label %partial.done.0
partial.drop.0:
  store i1 false, ptr %task.slot.11, align 1
  %tmp.8 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %task.slot.12, i32 0, i32 0
  store ptr %tmp.8, ptr %task.slot.37, align 1
  %task.reload.17 = load ptr, ptr %task.slot.37, align 1
  %tmp.9 = load i1, ptr %task.reload.17, align 1
  store i1 %tmp.9, ptr %task.slot.38, align 1
  %task.reload.18 = load i1, ptr %task.slot.38, align 1
  %tmp.10 = icmp eq i1 %task.reload.18, false
  store i1 %tmp.10, ptr %task.slot.39, align 1
  %task.reload.19 = load i1, ptr %task.slot.39, align 1
  br i1 %task.reload.19, label %partial.leaf.path.0.0.0, label %partial.leaf.done.0.0
partial.leaf.path.0.0.0:
  %tmp.11 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %task.slot.12, i32 0, i32 2
  store ptr %tmp.11, ptr %task.slot.40, align 1
  %tmp.12 = load i1, ptr %task.slot.13, align 1
  store i1 %tmp.12, ptr %task.slot.41, align 1
  %task.reload.20 = load i1, ptr %task.slot.41, align 1
  br i1 %task.reload.20, label %partial.leaf.run.0.0, label %partial.leaf.done.0.0
partial.leaf.run.0.0:
  store i1 false, ptr %task.slot.13, align 1
  %task.reload.21 = load ptr, ptr %task.slot.40, align 1
  call void @zil.task.drop(ptr %task.reload.21)
  br label %partial.leaf.done.0.0
partial.leaf.done.0.0:
  br label %partial.done.0
partial.done.0:
  %v.4 = add i32 0, 1
  store i32 %v.4, ptr %task.slot.42, align 1
  store i1 false, ptr %task.slot.11, align 1
  store i1 false, ptr %task.slot.13, align 1
  call void @zil.task.owner.unlink(ptr %task.owner.0)
  %tmp.13 = load i1, ptr %task.slot.9, align 1
  store i1 %tmp.13, ptr %task.slot.43, align 1
  %task.reload.22 = load i1, ptr %task.slot.43, align 1
  br i1 %task.reload.22, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %task.slot.9, align 1
  call void @zil.fn.1(ptr %task.slot.10)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %task.slot.9, align 1
  %task.reload.23 = load i32, ptr %task.slot.42, align 1
  store i32 %task.reload.23, ptr %task.result, align 1
  store i32 1, ptr %task.state, align 1
  %task.ready.24 = load i32, ptr %task.result, align 1
  %ready.25.tag = insertvalue { i32, i32 } zeroinitializer, i32 1, 0
  %ready.25.value = insertvalue { i32, i32 } %ready.25.tag, i32 %task.ready.24, 1
  ret { i32, i32 } %ready.25.value
bb.3:
  br label %zil.trap
bb.4:
  %tmp.14 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %task.slot.12, i32 0, i32 0
  store ptr %tmp.14, ptr %task.slot.44, align 1
  %task.reload.25 = load ptr, ptr %task.slot.44, align 1
  %tmp.15 = load i1, ptr %task.reload.25, align 1
  store i1 %tmp.15, ptr %task.slot.45, align 1
  %task.reload.26 = load i1, ptr %task.slot.45, align 1
  %tmp.16 = icmp ne i1 %task.reload.26, false
  store i1 %tmp.16, ptr %task.slot.46, align 1
  %task.reload.27 = load i1, ptr %task.slot.46, align 1
  br i1 %task.reload.27, label %zil.trap, label %checked.2
checked.2:
  %tmp.17 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %task.slot.12, i32 0, i32 2
  store ptr %tmp.17, ptr %task.slot.47, align 1
  %task.reload.28 = load ptr, ptr %task.slot.47, align 1
  %v.8 = load ptr, ptr %task.reload.28, align 8
  store ptr %v.8, ptr %task.slot.48, align 1
  store i1 false, ptr %task.slot.13, align 1
  call void @zil.task.owner.link(ptr %task.owner.7, ptr %task.owners.anchor)
  call void @zil.task.owner.unlink(ptr %task.owner.1)
  %tmp.18 = load i1, ptr %task.slot.11, align 1
  store i1 %tmp.18, ptr %task.slot.49, align 1
  %task.reload.29 = load i1, ptr %task.slot.49, align 1
  br i1 %task.reload.29, label %partial.drop.3, label %partial.done.3
partial.drop.3:
  store i1 false, ptr %task.slot.11, align 1
  %tmp.19 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %task.slot.12, i32 0, i32 0
  store ptr %tmp.19, ptr %task.slot.50, align 1
  %task.reload.30 = load ptr, ptr %task.slot.50, align 1
  %tmp.20 = load i1, ptr %task.reload.30, align 1
  store i1 %tmp.20, ptr %task.slot.51, align 1
  %task.reload.31 = load i1, ptr %task.slot.51, align 1
  %tmp.21 = icmp eq i1 %task.reload.31, false
  store i1 %tmp.21, ptr %task.slot.52, align 1
  %task.reload.32 = load i1, ptr %task.slot.52, align 1
  br i1 %task.reload.32, label %partial.leaf.path.3.0.0, label %partial.leaf.done.3.0
partial.leaf.path.3.0.0:
  %tmp.22 = getelementptr { i1, { i32, [0 x i8] }, ptr }, ptr %task.slot.12, i32 0, i32 2
  store ptr %tmp.22, ptr %task.slot.53, align 1
  %tmp.23 = load i1, ptr %task.slot.13, align 1
  store i1 %tmp.23, ptr %task.slot.54, align 1
  %task.reload.33 = load i1, ptr %task.slot.54, align 1
  br i1 %task.reload.33, label %partial.leaf.run.3.0, label %partial.leaf.done.3.0
partial.leaf.run.3.0:
  store i1 false, ptr %task.slot.13, align 1
  %task.reload.34 = load ptr, ptr %task.slot.53, align 1
  call void @zil.task.drop(ptr %task.reload.34)
  br label %partial.leaf.done.3.0
partial.leaf.done.3.0:
  br label %partial.done.3
partial.done.3:
  store i1 false, ptr %task.slot.14, align 1
  call void @zil.task.owner.unlink(ptr %task.owner.7)
  %task.reload.35 = load ptr, ptr %task.slot.48, align 1
  store ptr %task.reload.35, ptr %task.slot.15, align 8
  store i1 true, ptr %task.slot.14, align 1
  call void @zil.task.owner.link(ptr %task.owner.2, ptr %task.owners.anchor)
  store i1 false, ptr %task.slot.18, align 1
  store i1 false, ptr %task.slot.16, align 1
  call void @zil.task.owner.unlink(ptr %task.owner.2)
  %v.9 = load ptr, ptr %task.slot.15, align 8
  store ptr %v.9, ptr %task.slot.55, align 1
  store i1 false, ptr %task.slot.14, align 1
  call void @zil.task.owner.link(ptr %task.owner.8, ptr %task.owners.anchor)
  call void @zil.task.owner.unlink(ptr %task.owner.8)
  %task.reload.36 = load ptr, ptr %task.slot.55, align 1
  store ptr %task.reload.36, ptr %task.slot.17, align 8
  store i1 true, ptr %task.slot.16, align 1
  call void @zil.task.owner.link(ptr %task.owner.3, ptr %task.owners.anchor)
  call void @zil.task.owner.unlink(ptr %task.owner.0)
  %v.10 = load { i32 }, ptr %task.slot.10, align 4
  store { i32 } %v.10, ptr %task.slot.56, align 1
  store i1 false, ptr %task.slot.9, align 1
  call void @zil.task.owner.link(ptr %task.owner.9, ptr %task.owners.anchor)
  call void @zil.task.owner.unlink(ptr %task.owner.9)
  %task.reload.37 = load { i32 }, ptr %task.slot.56, align 1
  store { i32 } %task.reload.37, ptr %task.slot.19, align 4
  store i1 true, ptr %task.slot.18, align 1
  call void @zil.task.owner.link(ptr %task.owner.4, ptr %task.owners.anchor)
  call void @zil.task.owner.unlink(ptr %task.owner.3)
  %v.11 = load ptr, ptr %task.slot.17, align 8
  store ptr %v.11, ptr %task.slot.57, align 1
  store i1 false, ptr %task.slot.16, align 1
  call void @zil.task.owner.link(ptr %task.owner.10, ptr %task.owners.anchor)
  call void @zil.task.owner.unlink(ptr %task.owner.4)
  %v.12 = load { i32 }, ptr %task.slot.19, align 4
  store { i32 } %v.12, ptr %task.slot.58, align 1
  store i1 false, ptr %task.slot.18, align 1
  call void @zil.task.owner.link(ptr %task.owner.11, ptr %task.owners.anchor)
  call void @zil.task.owner.unlink(ptr %task.owner.10)
  call void @zil.task.owner.unlink(ptr %task.owner.11)
  %task.reload.38 = load ptr, ptr %task.slot.57, align 1
  %task.reload.39 = load { i32 }, ptr %task.slot.58, align 1
  %v.13 = call %zil_env_0 @zil.fn.4(ptr %task.reload.38, { i32 } %task.reload.39)
  store %zil_env_0 %v.13, ptr %task.slot.59, align 1
  call void @zil.task.owner.link(ptr %task.owner.12, ptr %task.owners.anchor)
  %tmp.24 = load i1, ptr %task.slot.18, align 1
  store i1 %tmp.24, ptr %task.slot.60, align 1
  %task.reload.40 = load i1, ptr %task.slot.60, align 1
  br i1 %task.reload.40, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %task.slot.18, align 1
  call void @zil.fn.1(ptr %task.slot.19)
  br label %resource.done.4
resource.done.4:
  %tmp.25 = load i1, ptr %task.slot.16, align 1
  store i1 %tmp.25, ptr %task.slot.61, align 1
  %task.reload.41 = load i1, ptr %task.slot.61, align 1
  br i1 %task.reload.41, label %async.resource.drop.5, label %async.resource.done.5
async.resource.drop.5:
  br label %async.drop.21.poll
async.drop.21.poll:
  %async.drop.21.ready = call i1 @zil.task.drop.poll(ptr %task.slot.17)
  br i1 %async.drop.21.ready, label %async.drop.21.after, label %async.drop.21.pending
async.drop.21.pending:
  store i32 22, ptr %task.pc, align 1
  store i32 0, ptr %task.state, align 1
  %async.drop.21.tag = insertvalue { i32, i32 } zeroinitializer, i32 0, 0
  %async.drop.21.value = insertvalue { i32, i32 } %async.drop.21.tag, i32 zeroinitializer, 1
  ret { i32, i32 } %async.drop.21.value
async.drop.21.after:
  store i1 false, ptr %task.slot.16, align 1
  br label %async.resource.done.5
async.resource.done.5:
  store i1 false, ptr %task.slot.20, align 1
  call void @zil.task.owner.unlink(ptr %task.owner.12)
  %task.reload.42 = load %zil_env_0, ptr %task.slot.59, align 1
  store %zil_env_0 %task.reload.42, ptr %task.slot.21, align 8
  store i1 true, ptr %task.slot.20, align 1
  call void @zil.task.owner.link(ptr %task.owner.5, ptr %task.owners.anchor)
  %v.14 = add i32 0, 0
  store i32 %v.14, ptr %task.slot.62, align 1
  %tmp.26 = load i1, ptr %task.slot.20, align 1
  store i1 %tmp.26, ptr %task.slot.63, align 1
  %task.reload.43 = load i1, ptr %task.slot.63, align 1
  br i1 %task.reload.43, label %closure.drop.6, label %closure.done.6
closure.drop.6:
  br label %async.drop.26.poll
async.drop.26.poll:
  %async.drop.26.ready = call i1 @zil.task.group.0.poll(ptr %task.slot.21)
  br i1 %async.drop.26.ready, label %async.drop.26.after, label %async.drop.26.pending
async.drop.26.pending:
  store i32 27, ptr %task.pc, align 1
  store i32 0, ptr %task.state, align 1
  %async.drop.26.tag = insertvalue { i32, i32 } zeroinitializer, i32 0, 0
  %async.drop.26.value = insertvalue { i32, i32 } %async.drop.26.tag, i32 zeroinitializer, 1
  ret { i32, i32 } %async.drop.26.value
async.drop.26.after:
  store i1 false, ptr %task.slot.20, align 1
  br label %closure.done.6
closure.done.6:
  call void @zil.task.owner.unlink(ptr %task.owner.5)
  store i1 false, ptr %task.slot.20, align 1
  store i1 false, ptr %task.slot.18, align 1
  store i1 false, ptr %task.slot.16, align 1
  %tmp.27 = load i1, ptr %task.slot.14, align 1
  store i1 %tmp.27, ptr %task.slot.64, align 1
  %task.reload.44 = load i1, ptr %task.slot.64, align 1
  br i1 %task.reload.44, label %async.resource.drop.7, label %async.resource.done.7
async.resource.drop.7:
  br label %async.drop.31.poll
async.drop.31.poll:
  %async.drop.31.ready = call i1 @zil.task.drop.poll(ptr %task.slot.15)
  br i1 %async.drop.31.ready, label %async.drop.31.after, label %async.drop.31.pending
async.drop.31.pending:
  store i32 32, ptr %task.pc, align 1
  store i32 0, ptr %task.state, align 1
  %async.drop.31.tag = insertvalue { i32, i32 } zeroinitializer, i32 0, 0
  %async.drop.31.value = insertvalue { i32, i32 } %async.drop.31.tag, i32 zeroinitializer, 1
  ret { i32, i32 } %async.drop.31.value
async.drop.31.after:
  store i1 false, ptr %task.slot.14, align 1
  br label %async.resource.done.7
async.resource.done.7:
  store i1 false, ptr %task.slot.14, align 1
  store i1 false, ptr %task.slot.11, align 1
  store i1 false, ptr %task.slot.13, align 1
  %tmp.28 = load i1, ptr %task.slot.9, align 1
  store i1 %tmp.28, ptr %task.slot.65, align 1
  %task.reload.45 = load i1, ptr %task.slot.65, align 1
  br i1 %task.reload.45, label %resource.drop.8, label %resource.done.8
resource.drop.8:
  store i1 false, ptr %task.slot.9, align 1
  call void @zil.fn.1(ptr %task.slot.10)
  br label %resource.done.8
resource.done.8:
  store i1 false, ptr %task.slot.9, align 1
  %task.reload.46 = load i32, ptr %task.slot.62, align 1
  store i32 %task.reload.46, ptr %task.result, align 1
  store i32 1, ptr %task.state, align 1
  %task.ready.47 = load i32, ptr %task.result, align 1
  %ready.48.tag = insertvalue { i32, i32 } zeroinitializer, i32 1, 0
  %ready.48.value = insertvalue { i32, i32 } %ready.48.tag, i32 %task.ready.47, 1
  ret { i32, i32 } %ready.48.value
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
define internal void @zil.task.init_into.5(ptr %frame, { i32 } %arg.0) {
entry:
  store %zil_task_frame_5 zeroinitializer, ptr %frame, align 1
  %header.0 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 0
  store ptr @zil.task.poll.5, ptr %header.0, align 1
  %header.1 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 1
  store ptr @zil.task.request_cancel.5, ptr %header.1, align 1
  %header.2 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 2
  store ptr @zil.task.poll_close.5, ptr %header.2, align 1
  %header.3 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 3
  store ptr @zil.task.finish_into.5, ptr %header.3, align 1
  %argument.8 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 8
  store { i32 } %arg.0, ptr %argument.8, align 1
  %task.owners.anchor = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 66
  %task.owner.0 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 67
  %task.owner.1 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 68
  %task.owner.2 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 69
  %task.owner.3 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 70
  %task.owner.4 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 71
  %task.owner.5 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 72
  %task.owner.6 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 73
  %task.owner.7 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 74
  %task.owner.8 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 75
  %task.owner.9 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 76
  %task.owner.10 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 77
  %task.owner.11 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 78
  %task.owner.12 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 79
  %anchor.prev.0 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owners.anchor, i32 0, i32 0
  store ptr %task.owners.anchor, ptr %anchor.prev.0, align 1
  %anchor.next.1 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owners.anchor, i32 0, i32 1
  store ptr %task.owners.anchor, ptr %anchor.next.1, align 1
  %owner.payload.0 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 10
  %owner.drop.0.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.0, i32 0, i32 2
  store ptr @zil.fn.1, ptr %owner.drop.0.2, align 1
  %owner.async.0.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.0, i32 0, i32 4
  store ptr null, ptr %owner.async.0.4.4, align 1
  %owner.async.0.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.0, i32 0, i32 5
  store ptr null, ptr %owner.async.0.5.5, align 1
  %owner.storage.0.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.0, i32 0, i32 3
  store ptr %owner.payload.0, ptr %owner.storage.0.3, align 1
  %owner.payload.1 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 12
  %owner.drop.1.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.1, i32 0, i32 2
  store ptr @zil.sum.drop.b2261691b0f3e538a5bbbb6b0b3c209de8659379ad87d4232e969b81fcd3d942, ptr %owner.drop.1.2, align 1
  %owner.async.1.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.1, i32 0, i32 4
  store ptr null, ptr %owner.async.1.4.4, align 1
  %owner.async.1.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.1, i32 0, i32 5
  store ptr null, ptr %owner.async.1.5.5, align 1
  %owner.storage.1.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.1, i32 0, i32 3
  store ptr %owner.payload.1, ptr %owner.storage.1.3, align 1
  %owner.payload.2 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 15
  %owner.drop.2.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.2, i32 0, i32 2
  store ptr @zil.task.drop, ptr %owner.drop.2.2, align 1
  %owner.async.2.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.2, i32 0, i32 4
  store ptr @zil.task.poll_close, ptr %owner.async.2.4.4, align 1
  %owner.async.2.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.2, i32 0, i32 5
  store ptr @zil.task.request_cancel, ptr %owner.async.2.5.5, align 1
  %owner.storage.2.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.2, i32 0, i32 3
  store ptr %owner.payload.2, ptr %owner.storage.2.3, align 1
  %owner.payload.3 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 17
  %owner.drop.3.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.3, i32 0, i32 2
  store ptr @zil.task.drop, ptr %owner.drop.3.2, align 1
  %owner.async.3.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.3, i32 0, i32 4
  store ptr @zil.task.poll_close, ptr %owner.async.3.4.4, align 1
  %owner.async.3.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.3, i32 0, i32 5
  store ptr @zil.task.request_cancel, ptr %owner.async.3.5.5, align 1
  %owner.storage.3.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.3, i32 0, i32 3
  store ptr %owner.payload.3, ptr %owner.storage.3.3, align 1
  %owner.payload.4 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 19
  %owner.drop.4.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.4, i32 0, i32 2
  store ptr @zil.fn.1, ptr %owner.drop.4.2, align 1
  %owner.async.4.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.4, i32 0, i32 4
  store ptr null, ptr %owner.async.4.4.4, align 1
  %owner.async.4.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.4, i32 0, i32 5
  store ptr null, ptr %owner.async.4.5.5, align 1
  %owner.storage.4.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.4, i32 0, i32 3
  store ptr %owner.payload.4, ptr %owner.storage.4.3, align 1
  %owner.payload.5 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 21
  %owner.drop.5.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.5, i32 0, i32 2
  store ptr null, ptr %owner.drop.5.2, align 1
  %owner.async.5.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.5, i32 0, i32 4
  store ptr @zil.task.group.0.poll, ptr %owner.async.5.4.4, align 1
  %owner.async.5.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.5, i32 0, i32 5
  store ptr @zil.task.group.0.request, ptr %owner.async.5.5.5, align 1
  %owner.storage.5.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.5, i32 0, i32 3
  store ptr %owner.payload.5, ptr %owner.storage.5.3, align 1
  %owner.payload.6 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 25
  %owner.drop.6.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.6, i32 0, i32 2
  store ptr @zil.sum.drop.b2261691b0f3e538a5bbbb6b0b3c209de8659379ad87d4232e969b81fcd3d942, ptr %owner.drop.6.2, align 1
  %owner.async.6.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.6, i32 0, i32 4
  store ptr null, ptr %owner.async.6.4.4, align 1
  %owner.async.6.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.6, i32 0, i32 5
  store ptr null, ptr %owner.async.6.5.5, align 1
  %owner.storage.6.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.6, i32 0, i32 3
  store ptr %owner.payload.6, ptr %owner.storage.6.3, align 1
  %owner.payload.7 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 48
  %owner.drop.7.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.7, i32 0, i32 2
  store ptr @zil.task.drop, ptr %owner.drop.7.2, align 1
  %owner.async.7.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.7, i32 0, i32 4
  store ptr @zil.task.poll_close, ptr %owner.async.7.4.4, align 1
  %owner.async.7.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.7, i32 0, i32 5
  store ptr @zil.task.request_cancel, ptr %owner.async.7.5.5, align 1
  %owner.storage.7.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.7, i32 0, i32 3
  store ptr %owner.payload.7, ptr %owner.storage.7.3, align 1
  %owner.payload.8 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 55
  %owner.drop.8.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.8, i32 0, i32 2
  store ptr @zil.task.drop, ptr %owner.drop.8.2, align 1
  %owner.async.8.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.8, i32 0, i32 4
  store ptr @zil.task.poll_close, ptr %owner.async.8.4.4, align 1
  %owner.async.8.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.8, i32 0, i32 5
  store ptr @zil.task.request_cancel, ptr %owner.async.8.5.5, align 1
  %owner.storage.8.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.8, i32 0, i32 3
  store ptr %owner.payload.8, ptr %owner.storage.8.3, align 1
  %owner.payload.9 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 56
  %owner.drop.9.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.9, i32 0, i32 2
  store ptr @zil.fn.1, ptr %owner.drop.9.2, align 1
  %owner.async.9.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.9, i32 0, i32 4
  store ptr null, ptr %owner.async.9.4.4, align 1
  %owner.async.9.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.9, i32 0, i32 5
  store ptr null, ptr %owner.async.9.5.5, align 1
  %owner.storage.9.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.9, i32 0, i32 3
  store ptr %owner.payload.9, ptr %owner.storage.9.3, align 1
  %owner.payload.10 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 57
  %owner.drop.10.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.10, i32 0, i32 2
  store ptr @zil.task.drop, ptr %owner.drop.10.2, align 1
  %owner.async.10.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.10, i32 0, i32 4
  store ptr @zil.task.poll_close, ptr %owner.async.10.4.4, align 1
  %owner.async.10.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.10, i32 0, i32 5
  store ptr @zil.task.request_cancel, ptr %owner.async.10.5.5, align 1
  %owner.storage.10.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.10, i32 0, i32 3
  store ptr %owner.payload.10, ptr %owner.storage.10.3, align 1
  %owner.payload.11 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 58
  %owner.drop.11.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.11, i32 0, i32 2
  store ptr @zil.fn.1, ptr %owner.drop.11.2, align 1
  %owner.async.11.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.11, i32 0, i32 4
  store ptr null, ptr %owner.async.11.4.4, align 1
  %owner.async.11.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.11, i32 0, i32 5
  store ptr null, ptr %owner.async.11.5.5, align 1
  %owner.storage.11.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.11, i32 0, i32 3
  store ptr %owner.payload.11, ptr %owner.storage.11.3, align 1
  %owner.payload.12 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 59
  %owner.drop.12.2 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.12, i32 0, i32 2
  store ptr null, ptr %owner.drop.12.2, align 1
  %owner.async.12.4.4 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.12, i32 0, i32 4
  store ptr @zil.task.group.0.poll, ptr %owner.async.12.4.4, align 1
  %owner.async.12.5.5 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.12, i32 0, i32 5
  store ptr @zil.task.group.0.request, ptr %owner.async.12.5.5, align 1
  %owner.storage.12.3 = getelementptr { ptr, ptr, ptr, ptr, ptr, ptr }, ptr %task.owner.12, i32 0, i32 3
  store ptr %owner.payload.12, ptr %owner.storage.12.3, align 1
  %input.owner.0 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 10
  store { i32 } %arg.0, ptr %input.owner.0, align 1
  %input.live.0 = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 9
  store i1 true, ptr %input.live.0, align 1
  call void @zil.task.owner.link(ptr %task.owner.0, ptr %task.owners.anchor)
  ret void
}
define internal ptr @zil.task.init_owned.5({ i32 } %arg.0) {
entry:
  %size.ptr = getelementptr %zil_task_frame_5, ptr null, i64 1
  %size = ptrtoint ptr %size.ptr to i64
  %frame = call ptr @calloc(i64 1, i64 %size)
  %allocated = icmp ne ptr %frame, null
  br i1 %allocated, label %init, label %return
init:
  call void @zil.task.init_into.5(ptr %frame, { i32 } %arg.0)
  %owner.destroy = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 3
  store ptr @zil.task.finish_destroy.5, ptr %owner.destroy, align 1
  br label %return
return:
  ret ptr %frame
}
define internal ptr @zil.task.init.5({ i32 } %arg.0) noinline {
entry:
  %size.ptr = getelementptr %zil_task_frame_5, ptr null, i64 1
  %size = ptrtoint ptr %size.ptr to i64
  %frame = call ptr @calloc(i64 1, i64 %size)
  %allocated = icmp ne ptr %frame, null
  br i1 %allocated, label %init, label %failure
init:
  call void @zil.task.init_into.5(ptr %frame, { i32 } %arg.0)
  %owner.destroy = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 3
  store ptr @zil.task.finish_destroy.5, ptr %owner.destroy, align 1
  br label %return
failure:
  %failed.input.0 = alloca { i32 }, align 8
  store { i32 } %arg.0, ptr %failed.input.0, align 1
  call void @zil.fn.1(ptr %failed.input.0)
  br label %return
return:
  ret ptr %frame
}
define internal void @zil.task.request_cancel.5(ptr %frame) {
entry:
  %cancel.state = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 4
  %cancel.previous = load i32, ptr %cancel.state, align 1
  store i32 4, ptr %cancel.state, align 1
  %cancel = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 6
  store i1 true, ptr %cancel, align 1
  %cancel.owners.anchor = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 66
  call void @zil.task.owner.request(ptr %cancel.owners.anchor)
  store i32 %cancel.previous, ptr %cancel.state, align 1
  ret void
}
define internal i1 @zil.task.poll_close.5(ptr %frame) {
entry:
  %state = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 4
  %current = load i32, ptr %state, align 1
  %active = icmp eq i32 %current, 4
  br i1 %active, label %busy.pending, label %close
close:
  store i32 4, ptr %state, align 1
  call void @zil.task.request_cancel.5(ptr %frame)
  %close.owners.anchor = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 66
  %close.owners.ready = call i1 @zil.task.owner.drain(ptr %close.owners.anchor)
  br i1 %close.owners.ready, label %owners.closed, label %pending
owners.closed:
  store i32 3, ptr %state, align 1
  ret i1 true
pending:
  store i32 5, ptr %state, align 1
  ret i1 false
busy.pending:
  ret i1 false
}
define internal void @zil.task.finish_into.5(ptr %frame) {
entry:
  %state = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 4
  %current = load i32, ptr %state, align 1
  %closed = icmp eq i32 %current, 3
  br i1 %closed, label %done, label %task.trap
done:
  ret void
task.trap:
  call void @llvm.trap()
  unreachable
}
define internal void @zil.task.finish_destroy.5(ptr %frame) {
entry:
  %state = getelementptr %zil_task_frame_5, ptr %frame, i32 0, i32 4
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
  br i1 %tmp.25, label %async.resource.drop.5, label %async.resource.done.5
async.resource.drop.5:
  call void @zil.task.drop(ptr %local.3)
  store i1 false, ptr %resource.live.3, align 1
  br label %async.resource.done.5
async.resource.done.5:
  store i1 false, ptr %resource.live.5, align 1
  store %zil_env_0 %v.13, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.14 = add i32 0, 0
  %tmp.26 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.26, label %closure.drop.6, label %closure.done.6
closure.drop.6:
  call void @zil.closure.drop.0(ptr %local.5)
  store i1 false, ptr %resource.live.5, align 1
  br label %closure.done.6
closure.done.6:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %tmp.27 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.27, label %async.resource.drop.7, label %async.resource.done.7
async.resource.drop.7:
  call void @zil.task.drop(ptr %local.2)
  store i1 false, ptr %resource.live.2, align 1
  br label %async.resource.done.7
async.resource.done.7:
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
define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.6()
  ret i32 %entry.result
}
