; ZIL verified LLVM module
; verified-fingerprint: 7e3294f2709a4b2f8b90050659439c7f422e6a6724fbdbabbaae52ffd7a30386
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

define i32 @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %local.2 = alloca %zil_env_0, align 8
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.1, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = getelementptr i8, ptr %local.0, i64 0
  %v.3 = add i8 0, 4
  %v.4 = call i1 @zil.bytes.push(ptr %v.2, i8 %v.3)
  %v.5 = getelementptr i8, ptr %local.1, i64 0
  %v.6 = add i8 0, 7
  %v.7 = call i1 @zil.bytes.push(ptr %v.5, i8 %v.6)
  %v.8 = getelementptr i8, ptr %local.0, i64 0
  %v.9 = insertvalue %zil_env_0 zeroinitializer, ptr %v.8, 0
  store %zil_env_0 %v.9, ptr %local.2, align 8
  %v.10 = getelementptr i8, ptr %local.2, i64 0
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = getelementptr i8, ptr %v.11, i64 0
  %v.13 = getelementptr i8, ptr %local.1, i64 0
  %tmp.0 = load ptr, ptr %v.13, align 8
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.13, i32 0, i32 1
  %tmp.2 = load i64, ptr %tmp.1, align 8
  %tmp.3 = insertvalue { ptr, i64 } poison, ptr %tmp.0, 0
  %v.14 = insertvalue { ptr, i64 } %tmp.3, i64 %tmp.2, 1
  %tmp.4 = extractvalue { ptr, i64 } %v.14, 0
  %v.15 = insertvalue { ptr, i64 } %v.14, ptr %tmp.4, 0
  %v.16 = call i32 @zil.fn.3(ptr %v.12, { ptr, i64 } %v.15)
  %v.17 = add i32 0, 7
  %tmp.5 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.0
checked.0:
  %tmp.7 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.7, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.8 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.8, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.18
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env, { ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca ptr, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = extractvalue %zil_env_0 %env, 0
  store ptr %v.0, ptr %local.1, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %v.2 = add i64 0, 32
  %v.3 = call i1 @zil.bytes.reserve(ptr %v.1, i64 %v.2)
  %v.4 = load { ptr, i64 }, ptr %local.0, align 8
  %v.5 = add i32 0, 0
  %tmp.0 = extractvalue { ptr, i64 } %v.4, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.4, 1
  %tmp.2 = sext i32 %v.5 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.6 = getelementptr i8, ptr %tmp.0, i64 %tmp.2
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = load i8, ptr %v.7, align 1
  %v.9 = zext i8 %v.8 to i32
  ret i32 %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(ptr %arg.0, { ptr, i64 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { ptr, i64 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.1, 0
  %v.2 = insertvalue { ptr, i64 } %v.1, ptr %tmp.0, 0
  %tmp.1 = load %zil_env_0, ptr %v.0, align 8
  %v.3 = call i32 @zil.fn.1(%zil_env_0 %tmp.1, { ptr, i64 } %v.2)
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3(ptr %arg.0, { ptr, i64 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { ptr, i64 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.2, 0
  %v.3 = insertvalue { ptr, i64 } %v.2, ptr %tmp.0, 0
  %v.4 = call i32 @zil.fn.2(ptr %v.1, { ptr, i64 } %v.3)
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
