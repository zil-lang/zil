; ZIL verified LLVM module
; verified-fingerprint: 43ca43ca2133091d7c1823a267443d2075beec1e88d59b4b61684062b6a27efb
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
%zil_env_1 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
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
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define %zil_env_0 @zil.fn.0(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = insertvalue %zil_env_0 zeroinitializer, i32 %v.0, 0
  ret %zil_env_0 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca %zil_env_1, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { ptr, i64, i64 }, %zil_env_1 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = add i32 0, 3
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = insertvalue %zil_env_1 zeroinitializer, i32 %v.1, 0
  store %zil_env_1 %v.2, ptr %local.1, align 4
  %v.3 = load %zil_env_1, ptr %local.1, align 4
  %v.4 = call { { ptr, i64, i64 }, %zil_env_1 } @zil.fn.4(%zil_env_1 %v.3)
  store { { ptr, i64, i64 }, %zil_env_1 } %v.4, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.5 = load { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.6 = call i32 @zil.fn.8({ { ptr, i64, i64 }, %zil_env_1 } %v.5)
  %v.7 = add i32 0, 23
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.6, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.3 = getelementptr { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.2, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.3)
  %tmp.4 = getelementptr { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.4)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(%zil_env_0 %env) {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = extractvalue %zil_env_0 %env, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  ret i32 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3(%zil_env_1 %env) {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = extractvalue %zil_env_1 %env, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  ret i32 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { ptr, i64, i64 }, %zil_env_1 } @zil.fn.4(%zil_env_1 %arg.0) {
entry:
  %local.0 = alloca %zil_env_1, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %local.2 = alloca %zil_env_1, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  store %zil_env_1 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = load %zil_env_1, ptr %local.0, align 4
  store %zil_env_1 %v.1, ptr %local.2, align 4
  %v.2 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.2, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = load %zil_env_1, ptr %local.2, align 4
  %v.4 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
%tmp.0 = insertvalue { { ptr, i64, i64 }, %zil_env_1 } zeroinitializer, { ptr, i64, i64 } %v.4, 0
%v.5 = insertvalue { { ptr, i64, i64 }, %zil_env_1 } %tmp.0, %zil_env_1 %v.3, 1
  %tmp.1 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.bytes.drop(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret { { ptr, i64, i64 }, %zil_env_1 } %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { ptr, i64, i64 }, %zil_env_0 } @zil.fn.5(%zil_env_0 %arg.0) {
entry:
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %local.2 = alloca %zil_env_0, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = load %zil_env_0, ptr %local.0, align 4
  store %zil_env_0 %v.1, ptr %local.2, align 4
  %v.2 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.2, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = load %zil_env_0, ptr %local.2, align 4
  %v.4 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
%tmp.0 = insertvalue { { ptr, i64, i64 }, %zil_env_0 } zeroinitializer, { ptr, i64, i64 } %v.4, 0
%v.5 = insertvalue { { ptr, i64, i64 }, %zil_env_0 } %tmp.0, %zil_env_0 %v.3, 1
  %tmp.1 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.bytes.drop(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret { { ptr, i64, i64 }, %zil_env_0 } %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6({ { ptr, i64, i64 }, %zil_env_0 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %local.1 = alloca ptr, align 8
  store { { ptr, i64, i64 }, %zil_env_0 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, i32 0, i32 1
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  store ptr %v.1, ptr %local.1, align 8
  %v.2 = load ptr, ptr %local.1, align 8
  %tmp.0 = load %zil_env_0, ptr %v.2, align 4
  %v.3 = call i32 @zil.fn.2(%zil_env_0 %tmp.0)
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.2 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.2)
  %tmp.3 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.3)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7({ { ptr, i64, i64 }, %zil_env_1 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 }, %zil_env_1 }, align 8
  %local.1 = alloca ptr, align 8
  store { { ptr, i64, i64 }, %zil_env_1 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.0, i32 0, i32 1
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  store ptr %v.1, ptr %local.1, align 8
  %v.2 = load ptr, ptr %local.1, align 8
  %tmp.0 = load %zil_env_1, ptr %v.2, align 4
  %v.3 = call i32 @zil.fn.3(%zil_env_1 %tmp.0)
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.2 = getelementptr { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.0, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.2)
  %tmp.3 = getelementptr { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.3)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.8({ { ptr, i64, i64 }, %zil_env_1 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 }, %zil_env_1 }, align 8
  %local.1 = alloca %zil_env_0, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { ptr, i64, i64 }, %zil_env_1 }, align 8
  store { { ptr, i64, i64 }, %zil_env_1 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = add i32 0, 20
  %v.1 = call %zil_env_0 @zil.fn.0(i32 %v.0)
  store %zil_env_0 %v.1, ptr %local.1, align 4
  %v.2 = load %zil_env_0, ptr %local.1, align 4
  %v.3 = call { { ptr, i64, i64 }, %zil_env_0 } @zil.fn.5(%zil_env_0 %v.2)
  store i1 false, ptr %resource.live.2, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %v.3, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.4 = load { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %v.4, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.5 = load { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.6 = call i32 @zil.fn.6({ { ptr, i64, i64 }, %zil_env_0 } %v.5)
  %v.7 = load { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store { { ptr, i64, i64 }, %zil_env_1 } %v.7, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.8 = load { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  %v.9 = call i32 @zil.fn.7({ { ptr, i64, i64 }, %zil_env_1 } %v.8)
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %tmp.2 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.2, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.3 = getelementptr { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.4, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.3)
  %tmp.4 = getelementptr { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.4, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.4)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.5 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.5, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.6 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.6)
  %tmp.7 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.7)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.8 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.8, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.9 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.9)
  %tmp.10 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.10)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.11 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.11, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.12 = getelementptr { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.0, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.12)
  %tmp.13 = getelementptr { { ptr, i64, i64 }, %zil_env_1 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.13)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
