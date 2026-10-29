; ZIL verified LLVM module
; verified-fingerprint: 8a5217a0146fd75fd075675518cd768c4f074ffbf43e39b5eed084d75453328c
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
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

define i32 @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  %local.2 = alloca %zil_env_0, align 1
  %local.3 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = add i64 0, 1
  %v.3 = call i1 @zil.bytes.reserve(ptr %v.1, i64 %v.2)
  %v.4 = xor i1 %v.3, true
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %v.5 = add i32 0, 1
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.5
bb.2:
  br label %bb.3
bb.3:
  %v.6 = getelementptr i8, ptr %local.0, i64 0
  %v.7 = add i8 0, 7
  %v.8 = call i1 @zil.bytes.push(ptr %v.6, i8 %v.7)
  %v.9 = xor i1 %v.8, true
  br i1 %v.9, label %bb.4, label %bb.5
bb.4:
  %v.10 = add i32 0, 2
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.10
bb.5:
  br label %bb.6
bb.6:
  %v.11 = getelementptr i8, ptr %local.0, i64 0
  %tmp.2 = load ptr, ptr %v.11, align 8
  %tmp.3 = getelementptr { ptr, i64, i64 }, ptr %v.11, i32 0, i32 1
  %tmp.4 = load i64, ptr %tmp.3, align 8
  %tmp.5 = insertvalue { ptr, i64 } poison, ptr %tmp.2, 0
  %v.12 = insertvalue { ptr, i64 } %tmp.5, i64 %tmp.4, 1
  store { ptr, i64 } %v.12, ptr %local.1, align 8
  %v.13 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.13, ptr %local.2, align 1
  %v.14 = load %zil_env_0, ptr %local.2, align 1
  %v.15 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.6 = extractvalue { ptr, i64 } %v.15, 0
  %v.16 = insertvalue { ptr, i64 } %v.15, ptr %tmp.6, 0
  %v.17 = call i32 @zil.fn.1(%zil_env_0 %v.14, { ptr, i64 } %v.16)
  store i32 %v.17, ptr %local.3, align 4
  %v.18 = getelementptr i8, ptr %local.0, i64 0
  %v.19 = add i64 0, 8
  %v.20 = call i1 @zil.bytes.reserve(ptr %v.18, i64 %v.19)
  %v.21 = xor i1 %v.20, true
  br i1 %v.21, label %bb.7, label %bb.8
bb.7:
  %v.22 = add i32 0, 3
  %tmp.7 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.7, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.22
bb.8:
  br label %bb.9
bb.9:
  %v.23 = load i32, ptr %local.3, align 4
  %v.24 = add i32 0, 7
  %tmp.8 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.23, i32 %v.24)
  %v.25 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  %tmp.10 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.10, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.25
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env, { ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { ptr, i64 }, ptr %local.0, align 8
  %v.1 = add i32 0, 0
  %tmp.0 = extractvalue { ptr, i64 } %v.0, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.0, 1
  %tmp.2 = sext i32 %v.1 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.2 = getelementptr i8, ptr %tmp.0, i64 %tmp.2
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = load i8, ptr %v.3, align 1
  %v.5 = zext i8 %v.4 to i32
  ret i32 %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
