; ZIL verified LLVM module
; verified-fingerprint: 72bcd119e8c8b9231d45fb96f366c6d29ed4b5a879904df4d1568ff5cd6701a3
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { ptr }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare ptr @realloc(ptr, i64)
declare void @free(ptr)

define internal ptr @zil.bytes.reallocate(ptr %old, i64 %size) {
entry:
  %result = call ptr @realloc(ptr %old, i64 %size)
  ret ptr %result
}
define internal void @zil.bytes.release(ptr %payload) {
entry:
  call void @free(ptr %payload)
  ret void
}

define internal i1 @zil.bytes.reserve(ptr %head, i64 %requested) {
entry:
  %negative = icmp slt i64 %requested, 0
  br i1 %negative, label %failed, label %capacity
capacity:
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %cap = load i64, ptr %cap.field, align 8
  %fits = icmp ule i64 %requested, %cap
  br i1 %fits, label %done, label %prepare
prepare:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %old = load ptr, ptr %ptr.field, align 8
  %next = call ptr @zil.bytes.reallocate(ptr %old, i64 %requested)
  %ok = icmp ne ptr %next, null
  br i1 %ok, label %publish, label %failed
publish:
  store ptr %next, ptr %ptr.field, align 8
  store i64 %requested, ptr %cap.field, align 8
  br label %done
done:
  ret i1 true
failed:
  ret i1 false
}
define internal i1 @zil.bytes.truncate(ptr %head, i64 %requested) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %len = load i64, ptr %len.field, align 8
  %nonnegative = icmp sge i64 %requested, 0
  %within = icmp ule i64 %requested, %len
  %valid = and i1 %nonnegative, %within
  br i1 %valid, label %publish, label %failed
publish:
  store i64 %requested, ptr %len.field, align 8
  ret i1 true
failed:
  ret i1 false
}
define internal i1 @zil.bytes.push(ptr %head, i8 %byte) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %len = load i64, ptr %len.field, align 8
  %cap = load i64, ptr %cap.field, align 8
  %room = icmp ult i64 %len, %cap
  br i1 %room, label %initialize, label %full
initialize:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %payload = load ptr, ptr %ptr.field, align 8
  %slot = getelementptr i8, ptr %payload, i64 %len
  store i8 %byte, ptr %slot, align 1
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret i1 true
full:
  ret i1 false
}
define internal void @zil.bytes.drop(ptr %head) {
entry:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %payload = load ptr, ptr %ptr.field, align 8
  call void @zil.bytes.release(ptr %payload)
  ret void
}
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i1 @zil.fn.0(ptr %arg.0, i8 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i8, align 1
  %local.2 = alloca i1, align 1
  store ptr %arg.0, ptr %local.0, align 8
  store i8 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = add i64 0, 1
  %v.2 = call i1 @zil.bytes.reserve(ptr %v.0, i64 %v.1)
  br i1 %v.2, label %bb.2, label %bb.1
bb.1:
  %v.3 = or i1 false, false
  store i1 %v.3, ptr %local.2, align 1
  br label %bb.3
bb.2:
  %v.4 = load ptr, ptr %local.0, align 8
  %v.5 = load i8, ptr %local.1, align 1
  %v.6 = call i1 @zil.bytes.push(ptr %v.4, i8 %v.5)
  store i1 %v.6, ptr %local.2, align 1
  br label %bb.3
bb.3:
  %v.7 = load i1, ptr %local.2, align 1
  ret i1 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.1({ ptr, i64, i64 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  store { ptr, i64, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  %local.1 = alloca %zil_env_0, align 8
  %local.2 = alloca %zil_env_0, align 8
  %local.3 = alloca i64, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = add i8 0, 65
  %v.3 = call i1 @zil.fn.0(ptr %v.1, i8 %v.2)
  %v.4 = xor i1 %v.3, true
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %v.5 = add i32 0, 8
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.5
bb.2:
  br label %bb.3
bb.3:
  %v.6 = getelementptr i8, ptr %local.0, i64 0
  %v.7 = insertvalue %zil_env_0 zeroinitializer, ptr %v.6, 0
  store %zil_env_0 %v.7, ptr %local.1, align 8
  %v.8 = load %zil_env_0, ptr %local.1, align 8
  store %zil_env_0 %v.8, ptr %local.2, align 8
  %v.9 = load %zil_env_0, ptr %local.2, align 8
  %v.10 = call i64 @zil.fn.3(%zil_env_0 %v.9)
  store i64 %v.10, ptr %local.3, align 8
  %v.11 = load { ptr, i64, i64 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.11, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.12 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.1({ ptr, i64, i64 } %v.12)
  %tmp.1 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.1, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.1
resource.done.1:
  %v.14 = load i64, ptr %local.3, align 8
  %tmp.2 = icmp slt i64 %v.14, -2147483648
  %tmp.3 = icmp sgt i64 %v.14, 2147483647
  %tmp.4 = or i1 %tmp.2, %tmp.3
  br i1 %tmp.4, label %zil.trap, label %checked.2
checked.2:
  %v.15 = trunc i64 %v.14 to i32
  %v.16 = add i32 0, 1
  %tmp.5 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.15, i32 %v.16)
  %v.17 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.3
checked.3:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.7 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.7, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.3(%zil_env_0 %env) {
entry:
  %local.0 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = extractvalue %zil_env_0 %env, 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.1, i32 0, i32 1
  %v.2 = load i64, ptr %tmp.0, align 8
  ret i64 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.2()
  ret i32 %entry.result
}
