; ZIL verified LLVM module
; verified-fingerprint: 966e0904009c9a2b086c3352414fc86b349f718b4981bb89a477610bf8a4bfd3
target triple = "x86_64-pc-windows-msvc"

%zil_env_0 = type { { i32 }, { ptr, i64, i64 }, i1, i1 }
%zil_env_1 = type {  }
%zil_env_2 = type {  }
%zil_env_3 = type {  }
%zil_env_4 = type {  }
%zil_env_5 = type {  }
%zil_env_6 = type {  }
%zil_env_7 = type {  }
%zil_env_8 = type {  }
%zil_env_9 = type {  }
%zil_env_10 = type { { ptr, i64, i64 }, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  call void @zil.fn.2(ptr %payload.0)
  br label %done.0
done.0:
  %flag.ptr.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  call void @zil.bytes.drop(ptr %payload.1)
  br label %done.1
done.1:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.2(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.3(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.4(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.5(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.6(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.7(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.8(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.9(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.10(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_10, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_10, ptr %env, i32 0, i32 0
  call void @zil.bytes.drop(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare ptr @GetProcessHeap()
declare ptr @HeapAlloc(ptr, i32, i64)
declare ptr @HeapReAlloc(ptr, i32, ptr, i64)
declare i32 @HeapFree(ptr, i32, ptr)

define internal ptr @zil.bytes.reallocate(ptr %old, i64 %size) {
entry:
  %heap = call ptr @GetProcessHeap()
  %heap.ok = icmp ne ptr %heap, null
  br i1 %heap.ok, label %choose, label %failed
choose:
  %empty = icmp eq ptr %old, null
  br i1 %empty, label %allocate, label %reallocate
allocate:
  %fresh = call ptr @HeapAlloc(ptr %heap, i32 0, i64 %size)
  ret ptr %fresh
reallocate:
  %grown = call ptr @HeapReAlloc(ptr %heap, i32 0, ptr %old, i64 %size)
  ret ptr %grown
failed:
  ret ptr null
}
define internal void @zil.bytes.release(ptr %payload) {
entry:
  %empty = icmp eq ptr %payload, null
  br i1 %empty, label %done, label %release
release:
  %heap = call ptr @GetProcessHeap()
  %released = call i32 @HeapFree(ptr %heap, i32 0, ptr %payload)
  %ok = icmp ne i32 %released, 0
  br i1 %ok, label %done, label %failed
failed:
  call void @llvm.trap()
  unreachable
done:
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
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)
declare { i8, i1 } @llvm.uadd.with.overflow.i8(i8, i8)

declare i32 @getchar()
define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i1, align 1
  %local.3 = alloca [2 x i8], align 1
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_0, align 8
  %local.5 = alloca i32, align 4
  %local.6 = alloca i32, align 4
  %local.7 = alloca i1, align 1
  %index.slot.22 = alloca [2 x i8], align 1
  %index.slot.51 = alloca [2 x i8], align 1
  %index.slot.61 = alloca [2 x i8], align 1
  br label %bb.0
bb.0:
  %v.0 = call i32 @zil.fn.10()
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = add i32 0, 0
  %v.3 = icmp ne i32 %v.1, %v.2
  br i1 %v.3, label %bb.1, label %bb.2
bb.1:
  %v.4 = load i32, ptr %local.0, align 4
  ret i32 %v.4
bb.2:
  br label %bb.3
bb.3:
  %v.5 = add i32 0, 0
  store i32 %v.5, ptr %local.1, align 4
  %v.6 = call i32 @getchar()
  store i32 %v.6, ptr %local.1, align 4
  %v.7 = load i32, ptr %local.1, align 4
  %v.8 = add i32 0, 48
  %v.9 = icmp slt i32 %v.7, %v.8
  br i1 %v.9, label %bb.4, label %bb.5
bb.4:
  %v.10 = or i1 false, true
  store i1 %v.10, ptr %local.2, align 1
  br label %bb.6
bb.5:
  %v.11 = load i32, ptr %local.1, align 4
  %v.12 = add i32 0, 51
  %v.13 = icmp sgt i32 %v.11, %v.12
  store i1 %v.13, ptr %local.2, align 1
  br label %bb.6
bb.6:
  %v.14 = load i1, ptr %local.2, align 1
  br i1 %v.14, label %bb.7, label %bb.8
bb.7:
  %v.15 = add i32 0, 9
  ret i32 %v.15
bb.8:
  br label %bb.9
bb.9:
  %v.16 = add i8 0, 65
  %v.17 = add i8 0, 97
%tmp.0 = insertvalue [2 x i8] zeroinitializer, i8 %v.16, 0
%v.18 = insertvalue [2 x i8] %tmp.0, i8 %v.17, 1
  store [2 x i8] %v.18, ptr %local.3, align 1
  %v.19 = call %zil_env_0 @zil.fn.3()
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_0 %v.19, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.20 = load [2 x i8], ptr %local.3, align 1
  %v.21 = add i32 0, 0
  %tmp.1 = sext i32 %v.21 to i64
  %tmp.2 = icmp ult i64 %tmp.1, 2
  br i1 %tmp.2, label %index.ok.0, label %zil.trap
index.ok.0:
  store [2 x i8] %v.20, ptr %index.slot.22, align 1
  %tmp.3 = getelementptr [2 x i8], ptr %index.slot.22, i64 0, i64 %tmp.1
  %v.22 = load i8, ptr %tmp.3, align 1
  %v.23 = zext i8 %v.22 to i32
  call void @zil.fn.5(i32 %v.23)
  %v.25 = add i32 0, 0
  store i32 %v.25, ptr %local.5, align 4
  br label %bb.10
bb.10:
  %v.26 = load i32, ptr %local.5, align 4
  %v.27 = load i32, ptr %local.1, align 4
  %v.28 = add i32 0, 48
  %tmp.4 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.27, i32 %v.28)
  %v.29 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  %v.30 = icmp slt i32 %v.26, %v.29
  br i1 %v.30, label %bb.11, label %bb.12
bb.11:
  %v.31 = getelementptr i8, ptr %local.4, i64 0
  %v.32 = getelementptr i8, ptr %v.31, i64 0
  %v.33 = getelementptr i8, ptr %v.32, i64 0
  %v.34 = getelementptr i8, ptr %local.3, i64 0
  %tmp.6 = icmp ugt i64 0, 2
  %tmp.7 = icmp ugt i64 2, 2
  %tmp.8 = or i1 %tmp.6, %tmp.7
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  %tmp.9 = getelementptr i8, ptr %v.34, i64 0
  %tmp.10 = sub i64 2, 0
  %tmp.11 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.9, 0
  %v.35 = insertvalue { ptr, i64 } %tmp.11, i64 %tmp.10, 1
  %tmp.12 = extractvalue { ptr, i64 } %v.35, 0
  %v.36 = insertvalue { ptr, i64 } %v.35, ptr %tmp.12, 0
  %tmp.13 = extractvalue { ptr, i64 } %v.36, 0
  %v.37 = insertvalue { ptr, i64 } %v.36, ptr %tmp.13, 0
  %v.38 = call i32 @zil.fn.12(ptr %v.33, { ptr, i64 } %v.37)
  store i32 %v.38, ptr %local.6, align 4
  %v.39 = load i32, ptr %local.6, align 4
  %v.40 = add i32 0, 0
  %v.41 = icmp slt i32 %v.39, %v.40
  br i1 %v.41, label %bb.13, label %bb.14
bb.12:
  %v.67 = add i32 0, 0
  %tmp.14 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.14, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.67
bb.13:
  %v.42 = add i32 0, 1
  %tmp.15 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.15, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.42
bb.14:
  br label %bb.15
bb.15:
  %v.43 = load i32, ptr %local.6, align 4
  %v.44 = load i32, ptr %local.5, align 4
  %v.45 = add i32 0, 1
  %tmp.16 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.44, i32 %v.45)
  %v.46 = extractvalue { i32, i1 } %tmp.16, 0
  %tmp.17 = extractvalue { i32, i1 } %tmp.16, 1
  br i1 %tmp.17, label %zil.trap, label %checked.5
checked.5:
  %v.47 = icmp ne i32 %v.43, %v.46
  br i1 %v.47, label %bb.16, label %bb.17
bb.16:
  %v.48 = or i1 false, true
  store i1 %v.48, ptr %local.7, align 1
  br label %bb.18
bb.17:
  %v.49 = load [2 x i8], ptr %local.3, align 1
  %v.50 = add i32 0, 1
  %tmp.18 = sext i32 %v.50 to i64
  %tmp.19 = icmp ult i64 %tmp.18, 2
  br i1 %tmp.19, label %index.ok.6, label %zil.trap
index.ok.6:
  store [2 x i8] %v.49, ptr %index.slot.51, align 1
  %tmp.20 = getelementptr [2 x i8], ptr %index.slot.51, i64 0, i64 %tmp.18
  %v.51 = load i8, ptr %tmp.20, align 1
  %v.52 = add i32 0, 98
  %v.53 = load i32, ptr %local.5, align 4
  %tmp.21 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.52, i32 %v.53)
  %v.54 = extractvalue { i32, i1 } %tmp.21, 0
  %tmp.22 = extractvalue { i32, i1 } %tmp.21, 1
  br i1 %tmp.22, label %zil.trap, label %checked.7
checked.7:
  %tmp.23 = icmp ugt i32 %v.54, 255
  br i1 %tmp.23, label %zil.trap, label %checked.8
checked.8:
  %v.55 = trunc i32 %v.54 to i8
  %v.56 = icmp ne i8 %v.51, %v.55
  store i1 %v.56, ptr %local.7, align 1
  br label %bb.18
bb.18:
  %v.57 = load i1, ptr %local.7, align 1
  br i1 %v.57, label %bb.19, label %bb.20
bb.19:
  %v.58 = add i32 0, 8
  %tmp.24 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.24, label %closure.drop.9, label %closure.done.9
closure.drop.9:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.9
closure.done.9:
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.58
bb.20:
  br label %bb.21
bb.21:
  %v.59 = load [2 x i8], ptr %local.3, align 1
  %v.60 = add i32 0, 0
  %tmp.25 = sext i32 %v.60 to i64
  %tmp.26 = icmp ult i64 %tmp.25, 2
  br i1 %tmp.26, label %index.ok.10, label %zil.trap
index.ok.10:
  store [2 x i8] %v.59, ptr %index.slot.61, align 1
  %tmp.27 = getelementptr [2 x i8], ptr %index.slot.61, i64 0, i64 %tmp.25
  %v.61 = load i8, ptr %tmp.27, align 1
  %v.62 = zext i8 %v.61 to i32
  call void @zil.fn.5(i32 %v.62)
  %v.64 = load i32, ptr %local.5, align 4
  %v.65 = add i32 0, 1
  %tmp.28 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.64, i32 %v.65)
  %v.66 = extractvalue { i32, i1 } %tmp.28, 0
  %tmp.29 = extractvalue { i32, i1 } %tmp.28, 1
  br i1 %tmp.29, label %zil.trap, label %checked.11
checked.11:
  store i32 %v.66, ptr %local.5, align 4
  br label %bb.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.2(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.5(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.3() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 72
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.4 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.5 = load { ptr, i64, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.4, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, { ptr, i64, i64 } %v.5, 1
  %tmp.2 = insertvalue %zil_env_0 %tmp.1, i1 true, 2
  %v.6 = insertvalue %zil_env_0 %tmp.2, i1 true, 3
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.bytes.drop(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.4 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.2(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_0 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32)
define void @zil.fn.5(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call i32 @putchar(i32 %v.0)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.6({ ptr, i64, i64 } %arg.0) {
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

define i64 @zil.fn.7({ ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca i64, align 8
  %local.2 = alloca i64, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 0
  store i64 %v.0, ptr %local.1, align 8
  %v.1 = add i64 0, 0
  store i64 %v.1, ptr %local.2, align 8
  br label %bb.1
bb.1:
  %v.2 = load i64, ptr %local.2, align 8
  %v.3 = load { ptr, i64 }, ptr %local.0, align 8
  %v.4 = extractvalue { ptr, i64 } %v.3, 1
  %v.5 = icmp slt i64 %v.2, %v.4
  br i1 %v.5, label %bb.2, label %bb.3
bb.2:
  %v.6 = load i64, ptr %local.1, align 8
  %v.7 = load { ptr, i64 }, ptr %local.0, align 8
  %v.8 = load i64, ptr %local.2, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.7, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.7, 1
  %tmp.2 = icmp uge i64 %v.8, %tmp.1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.9 = getelementptr i8, ptr %tmp.0, i64 %v.8
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = load i8, ptr %v.10, align 1
  %v.12 = zext i8 %v.11 to i64
  %tmp.3 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.6, i64 %v.12)
  %v.13 = extractvalue { i64, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i64, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  store i64 %v.13, ptr %local.1, align 8
  %v.14 = load i64, ptr %local.2, align 8
  %v.15 = add i64 0, 1
  %tmp.5 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.14, i64 %v.15)
  %v.16 = extractvalue { i64, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i64, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  store i64 %v.16, ptr %local.2, align 8
  br label %bb.1
bb.3:
  %v.17 = load i64, ptr %local.1, align 8
  ret i64 %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64 } @zil.fn.8({ ptr, i64 } %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca ptr, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.1, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %v.2 = load i32, ptr %v.1, align 4
  %v.3 = add i32 0, 10
  %tmp.0 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.5 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.6, ptr %v.0, align 4
  %v.7 = load { ptr, i64 }, ptr %local.0, align 8
  ret { ptr, i64 } %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.9(ptr %arg.0, i32 %arg.1, i32 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  store i32 %arg.2, ptr %local.2, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = load i32, ptr %v.1, align 4
  %v.3 = add i32 0, 10
  %tmp.0 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.5 = load i32, ptr %local.1, align 4
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.6, ptr %v.0, align 4
  %v.7 = load i32, ptr %local.2, align 4
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.10() {
entry:
  %local.0 = alloca [1 x i8], align 1
  %local.1 = alloca [2 x i8], align 1
  %local.2 = alloca [3 x i8], align 1
  %local.3 = alloca [4 x i8], align 1
  %local.4 = alloca [5 x i8], align 1
  %local.5 = alloca [6 x i8], align 1
  %local.6 = alloca %zil_env_1, align 1
  %local.7 = alloca i8, align 1
  %local.8 = alloca i64, align 8
  %local.9 = alloca i8, align 1
  %local.10 = alloca i64, align 8
  %local.11 = alloca [2 x { i8, i64 }], align 8
  %local.12 = alloca %zil_env_2, align 1
  %local.13 = alloca i1, align 1
  %local.14 = alloca [2 x [2 x i32]], align 4
  %local.15 = alloca %zil_env_3, align 1
  %local.16 = alloca i1, align 1
  %local.17 = alloca [3 x {}], align 1
  %local.18 = alloca %zil_env_4, align 1
  %local.19 = alloca [0 x i32], align 4
  %local.20 = alloca %zil_env_5, align 1
  %local.21 = alloca i32, align 4
  %local.22 = alloca [4 x i32], align 4
  %local.23 = alloca %zil_env_6, align 1
  %local.24 = alloca [2 x i8], align 1
  %local.25 = alloca { ptr, i64 }, align 8
  %local.26 = alloca %zil_env_7, align 1
  %local.27 = alloca %zil_env_8, align 1
  %local.28 = alloca i1, align 1
  %local.29 = alloca %zil_env_9, align 1
  %resource.live.30 = alloca i1, align 1
  store i1 false, ptr %resource.live.30, align 1
  %local.30 = alloca { ptr, i64, i64 }, align 8
  %resource.live.31 = alloca i1, align 1
  store i1 false, ptr %resource.live.31, align 1
  %local.31 = alloca %zil_env_10, align 8
  %resource.live.32 = alloca i1, align 1
  store i1 false, ptr %resource.live.32, align 1
  %local.32 = alloca %zil_env_10, align 8
  %index.slot.80 = alloca [2 x { i8, i64 }], align 8
  %index.slot.105 = alloca [2 x [2 x i32]], align 4
  %index.slot.107 = alloca [2 x i32], align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.32, align 1
  %v.0 = add i8 0, 1
%v.1 = insertvalue [1 x i8] zeroinitializer, i8 %v.0, 0
  store [1 x i8] %v.1, ptr %local.0, align 1
  %v.2 = add i8 0, 2
  %v.3 = add i8 0, 3
%tmp.0 = insertvalue [2 x i8] zeroinitializer, i8 %v.2, 0
%v.4 = insertvalue [2 x i8] %tmp.0, i8 %v.3, 1
  store [2 x i8] %v.4, ptr %local.1, align 1
  %v.5 = add i8 0, 4
  %v.6 = add i8 0, 5
  %v.7 = add i8 0, 6
%tmp.1 = insertvalue [3 x i8] zeroinitializer, i8 %v.5, 0
%tmp.2 = insertvalue [3 x i8] %tmp.1, i8 %v.6, 1
%v.8 = insertvalue [3 x i8] %tmp.2, i8 %v.7, 2
  store [3 x i8] %v.8, ptr %local.2, align 1
  %v.9 = add i8 0, 7
  %v.10 = add i8 0, 8
  %v.11 = add i8 0, 9
  %v.12 = add i8 0, 10
%tmp.3 = insertvalue [4 x i8] zeroinitializer, i8 %v.9, 0
%tmp.4 = insertvalue [4 x i8] %tmp.3, i8 %v.10, 1
%tmp.5 = insertvalue [4 x i8] %tmp.4, i8 %v.11, 2
%v.13 = insertvalue [4 x i8] %tmp.5, i8 %v.12, 3
  store [4 x i8] %v.13, ptr %local.3, align 1
  %v.14 = add i8 0, 11
  %v.15 = add i8 0, 12
  %v.16 = add i8 0, 13
  %v.17 = add i8 0, 14
  %v.18 = add i8 0, 15
%tmp.6 = insertvalue [5 x i8] zeroinitializer, i8 %v.14, 0
%tmp.7 = insertvalue [5 x i8] %tmp.6, i8 %v.15, 1
%tmp.8 = insertvalue [5 x i8] %tmp.7, i8 %v.16, 2
%tmp.9 = insertvalue [5 x i8] %tmp.8, i8 %v.17, 3
%v.19 = insertvalue [5 x i8] %tmp.9, i8 %v.18, 4
  store [5 x i8] %v.19, ptr %local.4, align 1
  %v.20 = add i8 0, 16
  %v.21 = add i8 0, 17
  %v.22 = add i8 0, 18
  %v.23 = add i8 0, 19
  %v.24 = add i8 0, 20
  %v.25 = add i8 0, 21
%tmp.10 = insertvalue [6 x i8] zeroinitializer, i8 %v.20, 0
%tmp.11 = insertvalue [6 x i8] %tmp.10, i8 %v.21, 1
%tmp.12 = insertvalue [6 x i8] %tmp.11, i8 %v.22, 2
%tmp.13 = insertvalue [6 x i8] %tmp.12, i8 %v.23, 3
%tmp.14 = insertvalue [6 x i8] %tmp.13, i8 %v.24, 4
%v.26 = insertvalue [6 x i8] %tmp.14, i8 %v.25, 5
  store [6 x i8] %v.26, ptr %local.5, align 1
  %v.27 = freeze %zil_env_1 zeroinitializer
  store %zil_env_1 %v.27, ptr %local.6, align 1
  %v.28 = load %zil_env_1, ptr %local.6, align 1
  %v.29 = getelementptr i8, ptr %local.0, i64 0
  %tmp.15 = icmp ugt i64 0, 1
  %tmp.16 = icmp ugt i64 1, 1
  %tmp.17 = or i1 %tmp.15, %tmp.16
  br i1 %tmp.17, label %zil.trap, label %checked.0
checked.0:
  %tmp.18 = getelementptr i8, ptr %v.29, i64 0
  %tmp.19 = sub i64 1, 0
  %tmp.20 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.18, 0
  %v.30 = insertvalue { ptr, i64 } %tmp.20, i64 %tmp.19, 1
  %tmp.21 = extractvalue { ptr, i64 } %v.30, 0
  %v.31 = insertvalue { ptr, i64 } %v.30, ptr %tmp.21, 0
  %tmp.22 = extractvalue { ptr, i64 } %v.31, 0
  %v.32 = insertvalue { ptr, i64 } %v.31, ptr %tmp.22, 0
  %v.33 = getelementptr i8, ptr %local.1, i64 0
  %tmp.23 = icmp ugt i64 0, 2
  %tmp.24 = icmp ugt i64 2, 2
  %tmp.25 = or i1 %tmp.23, %tmp.24
  br i1 %tmp.25, label %zil.trap, label %checked.1
checked.1:
  %tmp.26 = getelementptr i8, ptr %v.33, i64 0
  %tmp.27 = sub i64 2, 0
  %tmp.28 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.26, 0
  %v.34 = insertvalue { ptr, i64 } %tmp.28, i64 %tmp.27, 1
  %tmp.29 = extractvalue { ptr, i64 } %v.34, 0
  %v.35 = insertvalue { ptr, i64 } %v.34, ptr %tmp.29, 0
  %tmp.30 = extractvalue { ptr, i64 } %v.35, 0
  %v.36 = insertvalue { ptr, i64 } %v.35, ptr %tmp.30, 0
  %v.37 = getelementptr i8, ptr %local.2, i64 0
  %tmp.31 = icmp ugt i64 0, 3
  %tmp.32 = icmp ugt i64 3, 3
  %tmp.33 = or i1 %tmp.31, %tmp.32
  br i1 %tmp.33, label %zil.trap, label %checked.2
checked.2:
  %tmp.34 = getelementptr i8, ptr %v.37, i64 0
  %tmp.35 = sub i64 3, 0
  %tmp.36 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.34, 0
  %v.38 = insertvalue { ptr, i64 } %tmp.36, i64 %tmp.35, 1
  %tmp.37 = extractvalue { ptr, i64 } %v.38, 0
  %v.39 = insertvalue { ptr, i64 } %v.38, ptr %tmp.37, 0
  %tmp.38 = extractvalue { ptr, i64 } %v.39, 0
  %v.40 = insertvalue { ptr, i64 } %v.39, ptr %tmp.38, 0
  %v.41 = getelementptr i8, ptr %local.3, i64 0
  %tmp.39 = icmp ugt i64 0, 4
  %tmp.40 = icmp ugt i64 4, 4
  %tmp.41 = or i1 %tmp.39, %tmp.40
  br i1 %tmp.41, label %zil.trap, label %checked.3
checked.3:
  %tmp.42 = getelementptr i8, ptr %v.41, i64 0
  %tmp.43 = sub i64 4, 0
  %tmp.44 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.42, 0
  %v.42 = insertvalue { ptr, i64 } %tmp.44, i64 %tmp.43, 1
  %tmp.45 = extractvalue { ptr, i64 } %v.42, 0
  %v.43 = insertvalue { ptr, i64 } %v.42, ptr %tmp.45, 0
  %tmp.46 = extractvalue { ptr, i64 } %v.43, 0
  %v.44 = insertvalue { ptr, i64 } %v.43, ptr %tmp.46, 0
  %v.45 = getelementptr i8, ptr %local.4, i64 0
  %tmp.47 = icmp ugt i64 0, 5
  %tmp.48 = icmp ugt i64 5, 5
  %tmp.49 = or i1 %tmp.47, %tmp.48
  br i1 %tmp.49, label %zil.trap, label %checked.4
checked.4:
  %tmp.50 = getelementptr i8, ptr %v.45, i64 0
  %tmp.51 = sub i64 5, 0
  %tmp.52 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.50, 0
  %v.46 = insertvalue { ptr, i64 } %tmp.52, i64 %tmp.51, 1
  %tmp.53 = extractvalue { ptr, i64 } %v.46, 0
  %v.47 = insertvalue { ptr, i64 } %v.46, ptr %tmp.53, 0
  %tmp.54 = extractvalue { ptr, i64 } %v.47, 0
  %v.48 = insertvalue { ptr, i64 } %v.47, ptr %tmp.54, 0
  %v.49 = getelementptr i8, ptr %local.5, i64 0
  %tmp.55 = icmp ugt i64 0, 6
  %tmp.56 = icmp ugt i64 6, 6
  %tmp.57 = or i1 %tmp.55, %tmp.56
  br i1 %tmp.57, label %zil.trap, label %checked.5
checked.5:
  %tmp.58 = getelementptr i8, ptr %v.49, i64 0
  %tmp.59 = sub i64 6, 0
  %tmp.60 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.58, 0
  %v.50 = insertvalue { ptr, i64 } %tmp.60, i64 %tmp.59, 1
  %tmp.61 = extractvalue { ptr, i64 } %v.50, 0
  %v.51 = insertvalue { ptr, i64 } %v.50, ptr %tmp.61, 0
  %tmp.62 = extractvalue { ptr, i64 } %v.51, 0
  %v.52 = insertvalue { ptr, i64 } %v.51, ptr %tmp.62, 0
  %v.53 = call i64 @zil.fn.13(%zil_env_1 %v.28, { ptr, i64 } %v.32, { ptr, i64 } %v.36, { ptr, i64 } %v.40, { ptr, i64 } %v.44, { ptr, i64 } %v.48, { ptr, i64 } %v.52)
  %v.54 = add i64 0, 252
  %v.55 = icmp ne i64 %v.53, %v.54
  br i1 %v.55, label %bb.1, label %bb.2
bb.1:
  %v.56 = add i32 0, 11
  store i1 false, ptr %resource.live.32, align 1
  ret i32 %v.56
bb.2:
  br label %bb.3
bb.3:
  %v.57 = add i8 0, 1
  store i8 %v.57, ptr %local.7, align 1
  %v.58 = add i64 0, 2
  store i64 %v.58, ptr %local.8, align 8
  %v.59 = load i8, ptr %local.7, align 1
  %v.60 = load i64, ptr %local.8, align 8
%tmp.63 = insertvalue { i8, i64 } zeroinitializer, i8 %v.59, 0
%v.61 = insertvalue { i8, i64 } %tmp.63, i64 %v.60, 1
  %v.62 = add i8 0, 3
  store i8 %v.62, ptr %local.9, align 1
  %v.63 = add i64 0, 4
  store i64 %v.63, ptr %local.10, align 8
  %v.64 = load i8, ptr %local.9, align 1
  %v.65 = load i64, ptr %local.10, align 8
%tmp.64 = insertvalue { i8, i64 } zeroinitializer, i8 %v.64, 0
%v.66 = insertvalue { i8, i64 } %tmp.64, i64 %v.65, 1
%tmp.65 = insertvalue [2 x { i8, i64 }] zeroinitializer, { i8, i64 } %v.61, 0
%v.67 = insertvalue [2 x { i8, i64 }] %tmp.65, { i8, i64 } %v.66, 1
  store [2 x { i8, i64 }] %v.67, ptr %local.11, align 8
  %v.68 = freeze %zil_env_2 zeroinitializer
  store %zil_env_2 %v.68, ptr %local.12, align 1
  %v.69 = load %zil_env_2, ptr %local.12, align 1
  %v.70 = getelementptr i8, ptr %local.11, i64 0
  %tmp.66 = icmp ugt i64 0, 2
  %tmp.67 = icmp ugt i64 2, 2
  %tmp.68 = or i1 %tmp.66, %tmp.67
  br i1 %tmp.68, label %zil.trap, label %checked.6
checked.6:
  %tmp.69 = getelementptr { i8, i64 }, ptr %v.70, i64 0
  %tmp.70 = sub i64 2, 0
  %tmp.71 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.69, 0
  %v.71 = insertvalue { ptr, i64 } %tmp.71, i64 %tmp.70, 1
  %tmp.72 = extractvalue { ptr, i64 } %v.71, 0
  %v.72 = insertvalue { ptr, i64 } %v.71, ptr %tmp.72, 0
  %tmp.73 = extractvalue { ptr, i64 } %v.72, 0
  %v.73 = insertvalue { ptr, i64 } %v.72, ptr %tmp.73, 0
  %v.74 = call i64 @zil.fn.14(%zil_env_2 %v.69, { ptr, i64 } %v.73)
  %v.75 = add i64 0, 41
  %v.76 = icmp ne i64 %v.74, %v.75
  br i1 %v.76, label %bb.4, label %bb.5
bb.4:
  %v.77 = or i1 false, true
  store i1 %v.77, ptr %local.13, align 1
  br label %bb.6
bb.5:
  %v.78 = load [2 x { i8, i64 }], ptr %local.11, align 8
  %v.79 = add i32 0, 0
  %tmp.74 = sext i32 %v.79 to i64
  %tmp.75 = icmp ult i64 %tmp.74, 2
  br i1 %tmp.75, label %index.ok.7, label %zil.trap
index.ok.7:
  store [2 x { i8, i64 }] %v.78, ptr %index.slot.80, align 8
  %tmp.76 = getelementptr [2 x { i8, i64 }], ptr %index.slot.80, i64 0, i64 %tmp.74
  %v.80 = load { i8, i64 }, ptr %tmp.76, align 8
%v.81 = extractvalue { i8, i64 } %v.80, 0
  %v.82 = add i8 0, 1
  %v.83 = icmp ne i8 %v.81, %v.82
  store i1 %v.83, ptr %local.13, align 1
  br label %bb.6
bb.6:
  %v.84 = load i1, ptr %local.13, align 1
  br i1 %v.84, label %bb.7, label %bb.8
bb.7:
  %v.85 = add i32 0, 12
  store i1 false, ptr %resource.live.32, align 1
  ret i32 %v.85
bb.8:
  br label %bb.9
bb.9:
  %v.86 = add i32 0, 1
  %v.87 = add i32 0, 2
%tmp.77 = insertvalue [2 x i32] zeroinitializer, i32 %v.86, 0
%v.88 = insertvalue [2 x i32] %tmp.77, i32 %v.87, 1
  %v.89 = add i32 0, 3
  %v.90 = add i32 0, 4
%tmp.78 = insertvalue [2 x i32] zeroinitializer, i32 %v.89, 0
%v.91 = insertvalue [2 x i32] %tmp.78, i32 %v.90, 1
%tmp.79 = insertvalue [2 x [2 x i32]] zeroinitializer, [2 x i32] %v.88, 0
%v.92 = insertvalue [2 x [2 x i32]] %tmp.79, [2 x i32] %v.91, 1
  store [2 x [2 x i32]] %v.92, ptr %local.14, align 4
  %v.93 = freeze %zil_env_3 zeroinitializer
  store %zil_env_3 %v.93, ptr %local.15, align 1
  %v.94 = load %zil_env_3, ptr %local.15, align 1
  %v.95 = getelementptr i8, ptr %local.14, i64 0
  %tmp.80 = icmp ugt i64 0, 2
  %tmp.81 = icmp ugt i64 2, 2
  %tmp.82 = or i1 %tmp.80, %tmp.81
  br i1 %tmp.82, label %zil.trap, label %checked.8
checked.8:
  %tmp.83 = getelementptr [2 x i32], ptr %v.95, i64 0
  %tmp.84 = sub i64 2, 0
  %tmp.85 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.83, 0
  %v.96 = insertvalue { ptr, i64 } %tmp.85, i64 %tmp.84, 1
  %tmp.86 = extractvalue { ptr, i64 } %v.96, 0
  %v.97 = insertvalue { ptr, i64 } %v.96, ptr %tmp.86, 0
  %tmp.87 = extractvalue { ptr, i64 } %v.97, 0
  %v.98 = insertvalue { ptr, i64 } %v.97, ptr %tmp.87, 0
  %v.99 = call i32 @zil.fn.15(%zil_env_3 %v.94, { ptr, i64 } %v.98)
  %v.100 = add i32 0, 41
  %v.101 = icmp ne i32 %v.99, %v.100
  br i1 %v.101, label %bb.10, label %bb.11
bb.10:
  %v.102 = or i1 false, true
  store i1 %v.102, ptr %local.16, align 1
  br label %bb.12
bb.11:
  %v.103 = load [2 x [2 x i32]], ptr %local.14, align 4
  %v.104 = add i32 0, 0
  %tmp.88 = sext i32 %v.104 to i64
  %tmp.89 = icmp ult i64 %tmp.88, 2
  br i1 %tmp.89, label %index.ok.9, label %zil.trap
index.ok.9:
  store [2 x [2 x i32]] %v.103, ptr %index.slot.105, align 4
  %tmp.90 = getelementptr [2 x [2 x i32]], ptr %index.slot.105, i64 0, i64 %tmp.88
  %v.105 = load [2 x i32], ptr %tmp.90, align 4
  %v.106 = add i32 0, 1
  %tmp.91 = sext i32 %v.106 to i64
  %tmp.92 = icmp ult i64 %tmp.91, 2
  br i1 %tmp.92, label %index.ok.10, label %zil.trap
index.ok.10:
  store [2 x i32] %v.105, ptr %index.slot.107, align 4
  %tmp.93 = getelementptr [2 x i32], ptr %index.slot.107, i64 0, i64 %tmp.91
  %v.107 = load i32, ptr %tmp.93, align 4
  %v.108 = add i32 0, 2
  %v.109 = icmp ne i32 %v.107, %v.108
  store i1 %v.109, ptr %local.16, align 1
  br label %bb.12
bb.12:
  %v.110 = load i1, ptr %local.16, align 1
  br i1 %v.110, label %bb.13, label %bb.14
bb.13:
  %v.111 = add i32 0, 21
  store i1 false, ptr %resource.live.32, align 1
  ret i32 %v.111
bb.14:
  br label %bb.15
bb.15:
%tmp.94 = insertvalue [3 x {}] zeroinitializer, {} zeroinitializer, 0
%tmp.95 = insertvalue [3 x {}] %tmp.94, {} zeroinitializer, 1
%v.115 = insertvalue [3 x {}] %tmp.95, {} zeroinitializer, 2
  store [3 x {}] %v.115, ptr %local.17, align 1
  %v.116 = freeze %zil_env_4 zeroinitializer
  store %zil_env_4 %v.116, ptr %local.18, align 1
  %v.117 = load %zil_env_4, ptr %local.18, align 1
  %v.118 = getelementptr i8, ptr %local.17, i64 0
  %v.119 = add i32 0, 1
  %tmp.96 = sext i32 %v.119 to i64
  %tmp.97 = icmp ugt i64 %tmp.96, 3
  %tmp.98 = icmp ugt i64 3, 3
  %tmp.99 = or i1 %tmp.97, %tmp.98
  br i1 %tmp.99, label %zil.trap, label %checked.11
checked.11:
  %tmp.100 = getelementptr {}, ptr %v.118, i64 %tmp.96
  %tmp.101 = sub i64 3, %tmp.96
  %tmp.102 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.100, 0
  %v.120 = insertvalue { ptr, i64 } %tmp.102, i64 %tmp.101, 1
  %tmp.103 = extractvalue { ptr, i64 } %v.120, 0
  %v.121 = insertvalue { ptr, i64 } %v.120, ptr %tmp.103, 0
  %tmp.104 = extractvalue { ptr, i64 } %v.121, 0
  %v.122 = insertvalue { ptr, i64 } %v.121, ptr %tmp.104, 0
  %v.123 = call i64 @zil.fn.16(%zil_env_4 %v.117, { ptr, i64 } %v.122)
  %v.124 = add i64 0, 2
  %v.125 = icmp ne i64 %v.123, %v.124
  br i1 %v.125, label %bb.16, label %bb.17
bb.16:
  %v.126 = add i32 0, 13
  store i1 false, ptr %resource.live.32, align 1
  ret i32 %v.126
bb.17:
  br label %bb.18
bb.18:
%v.127 = freeze [0 x i32] zeroinitializer
  store [0 x i32] %v.127, ptr %local.19, align 4
  %v.128 = freeze %zil_env_5 zeroinitializer
  store %zil_env_5 %v.128, ptr %local.20, align 1
  %v.129 = load %zil_env_5, ptr %local.20, align 1
  %v.130 = getelementptr i8, ptr %local.19, i64 0
  %tmp.105 = icmp ugt i64 0, 0
  %tmp.106 = icmp ugt i64 0, 0
  %tmp.107 = or i1 %tmp.105, %tmp.106
  br i1 %tmp.107, label %zil.trap, label %checked.12
checked.12:
  %tmp.108 = getelementptr i32, ptr %v.130, i64 0
  %tmp.109 = sub i64 0, 0
  %tmp.110 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.108, 0
  %v.131 = insertvalue { ptr, i64 } %tmp.110, i64 %tmp.109, 1
  %tmp.111 = extractvalue { ptr, i64 } %v.131, 0
  %v.132 = insertvalue { ptr, i64 } %v.131, ptr %tmp.111, 0
  %tmp.112 = extractvalue { ptr, i64 } %v.132, 0
  %v.133 = insertvalue { ptr, i64 } %v.132, ptr %tmp.112, 0
  %v.134 = call i64 @zil.fn.17(%zil_env_5 %v.129, { ptr, i64 } %v.133)
  %v.135 = add i64 0, 0
  %v.136 = icmp ne i64 %v.134, %v.135
  br i1 %v.136, label %bb.19, label %bb.20
bb.19:
  %v.137 = add i32 0, 14
  store i1 false, ptr %resource.live.32, align 1
  ret i32 %v.137
bb.20:
  br label %bb.21
bb.21:
  %v.138 = add i32 0, 0
  store i32 %v.138, ptr %local.21, align 4
  %v.139 = add i32 0, 4
  %v.140 = add i32 0, 7
  %v.141 = add i32 0, 9
  %v.142 = add i32 0, 12
%tmp.113 = insertvalue [4 x i32] zeroinitializer, i32 %v.139, 0
%tmp.114 = insertvalue [4 x i32] %tmp.113, i32 %v.140, 1
%tmp.115 = insertvalue [4 x i32] %tmp.114, i32 %v.141, 2
%v.143 = insertvalue [4 x i32] %tmp.115, i32 %v.142, 3
  store [4 x i32] %v.143, ptr %local.22, align 4
  %v.144 = freeze %zil_env_6 zeroinitializer
  store %zil_env_6 %v.144, ptr %local.23, align 1
  %v.145 = load %zil_env_6, ptr %local.23, align 1
  %v.146 = getelementptr i8, ptr %local.22, i64 0
  %tmp.116 = icmp ugt i64 0, 4
  %tmp.117 = icmp ugt i64 4, 4
  %tmp.118 = or i1 %tmp.116, %tmp.117
  br i1 %tmp.118, label %zil.trap, label %checked.13
checked.13:
  %tmp.119 = getelementptr i32, ptr %v.146, i64 0
  %tmp.120 = sub i64 4, 0
  %tmp.121 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.119, 0
  %v.147 = insertvalue { ptr, i64 } %tmp.121, i64 %tmp.120, 1
  %tmp.122 = extractvalue { ptr, i64 } %v.147, 0
  %v.148 = insertvalue { ptr, i64 } %v.147, ptr %tmp.122, 0
  %tmp.123 = extractvalue { ptr, i64 } %v.148, 0
  %v.149 = insertvalue { ptr, i64 } %v.148, ptr %tmp.123, 0
  %v.150 = getelementptr i8, ptr %local.21, i64 0
  %v.151 = getelementptr i8, ptr %v.150, i64 0
  %v.152 = getelementptr i8, ptr %v.151, i64 0
  %v.153 = call { ptr, i64 } @zil.fn.8({ ptr, i64 } %v.149, ptr %v.152)
  %v.154 = getelementptr i8, ptr %local.21, i64 0
  %v.155 = getelementptr i8, ptr %v.154, i64 0
  %v.156 = getelementptr i8, ptr %v.155, i64 0
  %v.157 = add i32 0, 2
  %v.158 = add i32 0, 1
  %v.159 = call i32 @zil.fn.9(ptr %v.156, i32 %v.157, i32 %v.158)
  %v.160 = getelementptr i8, ptr %local.21, i64 0
  %v.161 = getelementptr i8, ptr %v.160, i64 0
  %v.162 = getelementptr i8, ptr %v.161, i64 0
  %v.163 = add i32 0, 3
  %v.164 = add i32 0, 3
  %v.165 = call i32 @zil.fn.9(ptr %v.162, i32 %v.163, i32 %v.164)
  %tmp.124 = extractvalue { ptr, i64 } %v.153, 0
  %tmp.125 = extractvalue { ptr, i64 } %v.153, 1
  %tmp.126 = sext i32 %v.159 to i64
  %tmp.127 = sext i32 %v.165 to i64
  %tmp.128 = icmp ugt i64 %tmp.126, %tmp.127
  %tmp.129 = icmp ugt i64 %tmp.127, %tmp.125
  %tmp.130 = or i1 %tmp.128, %tmp.129
  br i1 %tmp.130, label %zil.trap, label %checked.14
checked.14:
  %tmp.131 = getelementptr i32, ptr %tmp.124, i64 %tmp.126
  %tmp.132 = sub i64 %tmp.127, %tmp.126
  %tmp.133 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.131, 0
  %v.166 = insertvalue { ptr, i64 } %tmp.133, i64 %tmp.132, 1
  %tmp.134 = extractvalue { ptr, i64 } %v.166, 0
  %v.167 = insertvalue { ptr, i64 } %v.166, ptr %tmp.134, 0
  %tmp.135 = extractvalue { ptr, i64 } %v.167, 0
  %v.168 = insertvalue { ptr, i64 } %v.167, ptr %tmp.135, 0
  %v.169 = call i32 @zil.fn.18(%zil_env_6 %v.145, { ptr, i64 } %v.168)
  %v.170 = add i32 0, 9
  %v.171 = icmp ne i32 %v.169, %v.170
  br i1 %v.171, label %bb.22, label %bb.23
bb.22:
  %v.172 = add i32 0, 15
  store i1 false, ptr %resource.live.32, align 1
  ret i32 %v.172
bb.23:
  br label %bb.24
bb.24:
  %v.173 = load i32, ptr %local.21, align 4
  %v.174 = add i32 0, 123
  %v.175 = icmp ne i32 %v.173, %v.174
  br i1 %v.175, label %bb.25, label %bb.26
bb.25:
  %v.176 = add i32 0, 16
  store i1 false, ptr %resource.live.32, align 1
  ret i32 %v.176
bb.26:
  br label %bb.27
bb.27:
  %v.177 = add i8 0, 40
  %v.178 = add i8 0, 1
%tmp.136 = insertvalue [2 x i8] zeroinitializer, i8 %v.177, 0
%v.179 = insertvalue [2 x i8] %tmp.136, i8 %v.178, 1
  store [2 x i8] %v.179, ptr %local.24, align 1
  %v.180 = getelementptr i8, ptr %local.24, i64 0
  %tmp.137 = icmp ugt i64 0, 2
  %tmp.138 = icmp ugt i64 2, 2
  %tmp.139 = or i1 %tmp.137, %tmp.138
  br i1 %tmp.139, label %zil.trap, label %checked.15
checked.15:
  %tmp.140 = getelementptr i8, ptr %v.180, i64 0
  %tmp.141 = sub i64 2, 0
  %tmp.142 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.140, 0
  %v.181 = insertvalue { ptr, i64 } %tmp.142, i64 %tmp.141, 1
  %tmp.143 = extractvalue { ptr, i64 } %v.181, 0
  %v.182 = insertvalue { ptr, i64 } %v.181, ptr %tmp.143, 0
  store { ptr, i64 } %v.182, ptr %local.25, align 8
  %v.183 = freeze %zil_env_7 zeroinitializer
  store %zil_env_7 %v.183, ptr %local.26, align 1
  %v.184 = getelementptr i8, ptr %local.26, i64 0
  %v.185 = getelementptr i8, ptr %v.184, i64 0
  %v.186 = getelementptr i8, ptr %v.185, i64 0
  %v.187 = load { ptr, i64 }, ptr %local.25, align 8
  %tmp.144 = extractvalue { ptr, i64 } %v.187, 0
  %v.188 = insertvalue { ptr, i64 } %v.187, ptr %tmp.144, 0
  %v.189 = call i32 @zil.fn.20(ptr %v.186, { ptr, i64 } %v.188)
  %v.190 = add i32 0, 41
  %v.191 = icmp ne i32 %v.189, %v.190
  br i1 %v.191, label %bb.28, label %bb.29
bb.28:
  %v.192 = add i32 0, 17
  store i1 false, ptr %resource.live.32, align 1
  ret i32 %v.192
bb.29:
  br label %bb.30
bb.30:
  %v.193 = freeze %zil_env_8 zeroinitializer
  store %zil_env_8 %v.193, ptr %local.27, align 1
  %v.194 = load %zil_env_8, ptr %local.27, align 1
  %v.195 = load { ptr, i64 }, ptr %local.25, align 8
  %tmp.145 = extractvalue { ptr, i64 } %v.195, 0
  %v.196 = insertvalue { ptr, i64 } %v.195, ptr %tmp.145, 0
  %tmp.146 = extractvalue { ptr, i64 } %v.196, 0
  %v.197 = insertvalue { ptr, i64 } %v.196, ptr %tmp.146, 0
  %v.198 = call i32 @zil.fn.21(%zil_env_8 %v.194, { ptr, i64 } %v.197)
  %v.199 = add i32 0, 41
  %v.200 = icmp ne i32 %v.198, %v.199
  br i1 %v.200, label %bb.31, label %bb.32
bb.31:
  %v.201 = or i1 false, true
  store i1 %v.201, ptr %local.28, align 1
  br label %bb.33
bb.32:
  %v.202 = load %zil_env_8, ptr %local.27, align 1
  %v.203 = load { ptr, i64 }, ptr %local.25, align 8
  %tmp.147 = extractvalue { ptr, i64 } %v.203, 0
  %v.204 = insertvalue { ptr, i64 } %v.203, ptr %tmp.147, 0
  %tmp.148 = extractvalue { ptr, i64 } %v.204, 0
  %v.205 = insertvalue { ptr, i64 } %v.204, ptr %tmp.148, 0
  %v.206 = call i32 @zil.fn.21(%zil_env_8 %v.202, { ptr, i64 } %v.205)
  %v.207 = add i32 0, 42
  %v.208 = icmp ne i32 %v.206, %v.207
  store i1 %v.208, ptr %local.28, align 1
  br label %bb.33
bb.33:
  %v.209 = load i1, ptr %local.28, align 1
  br i1 %v.209, label %bb.34, label %bb.35
bb.34:
  %v.210 = add i32 0, 18
  store i1 false, ptr %resource.live.32, align 1
  ret i32 %v.210
bb.35:
  br label %bb.36
bb.36:
  %v.211 = freeze %zil_env_9 zeroinitializer
  store %zil_env_9 %v.211, ptr %local.29, align 1
  %v.212 = getelementptr i8, ptr %local.29, i64 0
  %v.213 = getelementptr i8, ptr %v.212, i64 0
  %v.214 = getelementptr i8, ptr %v.213, i64 0
  %v.215 = getelementptr i8, ptr %local.24, i64 0
  %tmp.149 = icmp ugt i64 0, 2
  %tmp.150 = icmp ugt i64 2, 2
  %tmp.151 = or i1 %tmp.149, %tmp.150
  br i1 %tmp.151, label %zil.trap, label %checked.16
checked.16:
  %tmp.152 = getelementptr i8, ptr %v.215, i64 0
  %tmp.153 = sub i64 2, 0
  %tmp.154 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.152, 0
  %v.216 = insertvalue { ptr, i64 } %tmp.154, i64 %tmp.153, 1
  %tmp.155 = extractvalue { ptr, i64 } %v.216, 0
  %v.217 = insertvalue { ptr, i64 } %v.216, ptr %tmp.155, 0
  %tmp.156 = extractvalue { ptr, i64 } %v.217, 0
  %v.218 = insertvalue { ptr, i64 } %v.217, ptr %tmp.156, 0
  %v.219 = call i32 @zil.fn.23(ptr %v.214, { ptr, i64 } %v.218)
  %v.220 = add i32 0, 43
  %v.221 = icmp ne i32 %v.219, %v.220
  br i1 %v.221, label %bb.37, label %bb.38
bb.37:
  %v.222 = add i32 0, 19
  store i1 false, ptr %resource.live.32, align 1
  ret i32 %v.222
bb.38:
  br label %bb.39
bb.39:
  %v.223 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.30, align 1
  store { ptr, i64, i64 } %v.223, ptr %local.30, align 8
  store i1 true, ptr %resource.live.30, align 1
  %v.224 = load { ptr, i64, i64 }, ptr %local.30, align 8
  store i1 false, ptr %resource.live.30, align 1
  %tmp.157 = insertvalue %zil_env_10 zeroinitializer, { ptr, i64, i64 } %v.224, 0
  %v.225 = insertvalue %zil_env_10 %tmp.157, i1 true, 1
  store i1 false, ptr %resource.live.31, align 1
  store %zil_env_10 %v.225, ptr %local.31, align 8
  store i1 true, ptr %resource.live.31, align 1
  %v.226 = load %zil_env_10, ptr %local.31, align 8
  store i1 false, ptr %resource.live.31, align 1
  store %zil_env_10 %v.226, ptr %local.32, align 8
  store i1 true, ptr %resource.live.32, align 1
  %v.227 = getelementptr i8, ptr %local.24, i64 0
  %tmp.158 = icmp ugt i64 0, 2
  %tmp.159 = icmp ugt i64 2, 2
  %tmp.160 = or i1 %tmp.158, %tmp.159
  br i1 %tmp.160, label %zil.trap, label %checked.17
checked.17:
  %tmp.161 = getelementptr i8, ptr %v.227, i64 0
  %tmp.162 = sub i64 2, 0
  %tmp.163 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.161, 0
  %v.228 = insertvalue { ptr, i64 } %tmp.163, i64 %tmp.162, 1
  %tmp.164 = extractvalue { ptr, i64 } %v.228, 0
  %v.229 = insertvalue { ptr, i64 } %v.228, ptr %tmp.164, 0
  %tmp.165 = extractvalue { ptr, i64 } %v.229, 0
  %v.230 = insertvalue { ptr, i64 } %v.229, ptr %tmp.165, 0
  %v.231 = load %zil_env_10, ptr %local.32, align 8
  store i1 false, ptr %resource.live.32, align 1
  %v.232 = call i32 @zil.fn.25(%zil_env_10 %v.231, { ptr, i64 } %v.230)
  %v.233 = add i32 0, 43
  %v.234 = icmp ne i32 %v.232, %v.233
  %tmp.166 = load i1, ptr %resource.live.32, align 1
  br i1 %tmp.166, label %closure.drop.18, label %closure.done.18
closure.drop.18:
  store i1 false, ptr %resource.live.32, align 1
  call void @zil.closure.drop.10(ptr %local.32)
  br label %closure.done.18
closure.done.18:
  br i1 %v.234, label %bb.40, label %bb.41
bb.40:
  %v.235 = add i32 0, 20
  store i1 false, ptr %resource.live.32, align 1
  %tmp.167 = load i1, ptr %resource.live.31, align 1
  br i1 %tmp.167, label %closure.drop.19, label %closure.done.19
closure.drop.19:
  store i1 false, ptr %resource.live.31, align 1
  call void @zil.closure.drop.10(ptr %local.31)
  br label %closure.done.19
closure.done.19:
  store i1 false, ptr %resource.live.31, align 1
  %tmp.168 = load i1, ptr %resource.live.30, align 1
  br i1 %tmp.168, label %resource.drop.20, label %resource.done.20
resource.drop.20:
  store i1 false, ptr %resource.live.30, align 1
  call void @zil.bytes.drop(ptr %local.30)
  br label %resource.done.20
resource.done.20:
  store i1 false, ptr %resource.live.30, align 1
  ret i32 %v.235
bb.41:
  br label %bb.42
bb.42:
  %v.236 = add i32 0, 0
  store i1 false, ptr %resource.live.32, align 1
  %tmp.169 = load i1, ptr %resource.live.31, align 1
  br i1 %tmp.169, label %closure.drop.21, label %closure.done.21
closure.drop.21:
  store i1 false, ptr %resource.live.31, align 1
  call void @zil.closure.drop.10(ptr %local.31)
  br label %closure.done.21
closure.done.21:
  store i1 false, ptr %resource.live.31, align 1
  %tmp.170 = load i1, ptr %resource.live.30, align 1
  br i1 %tmp.170, label %resource.drop.22, label %resource.done.22
resource.drop.22:
  store i1 false, ptr %resource.live.30, align 1
  call void @zil.bytes.drop(ptr %local.30)
  br label %resource.done.22
resource.done.22:
  store i1 false, ptr %resource.live.30, align 1
  ret i32 %v.236
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.11(ptr %env, { ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca ptr, align 8
  %local.2 = alloca ptr, align 8
  %local.3 = alloca i64, align 8
  %local.4 = alloca i64, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.1, align 8
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.2, align 8
  %v.2 = load ptr, ptr %local.1, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.0, align 4
  %v.4 = load ptr, ptr %local.2, align 8
  %v.5 = load ptr, ptr %local.2, align 8
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.5, i32 0, i32 1
  %v.6 = load i64, ptr %tmp.1, align 8
  %v.7 = add i64 0, 1
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.6, i64 %v.7)
  %v.8 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.9 = call i1 @zil.bytes.reserve(ptr %v.4, i64 %v.8)
  %v.10 = xor i1 %v.9, true
  br i1 %v.10, label %bb.1, label %bb.2
bb.1:
  %v.11 = add i32 0, -1
  ret i32 %v.11
bb.2:
  br label %bb.3
bb.3:
  %v.12 = load ptr, ptr %local.2, align 8
  %v.13 = load { ptr, i64 }, ptr %local.0, align 8
  %v.14 = add i32 0, 0
  %tmp.4 = extractvalue { ptr, i64 } %v.13, 0
  %tmp.5 = extractvalue { ptr, i64 } %v.13, 1
  %tmp.6 = sext i32 %v.14 to i64
  %tmp.7 = icmp uge i64 %tmp.6, %tmp.5
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.15 = getelementptr i8, ptr %tmp.4, i64 %tmp.6
  %v.16 = getelementptr i8, ptr %v.15, i64 0
  %v.17 = load i8, ptr %v.16, align 1
  %v.18 = call i1 @zil.bytes.push(ptr %v.12, i8 %v.17)
  %v.19 = xor i1 %v.18, true
  br i1 %v.19, label %bb.4, label %bb.5
bb.4:
  %v.20 = add i32 0, -2
  ret i32 %v.20
bb.5:
  br label %bb.6
bb.6:
  %v.21 = add i64 0, 0
  store i64 %v.21, ptr %local.3, align 8
  br label %bb.7
bb.7:
  %v.22 = load i64, ptr %local.3, align 8
  %v.23 = load { ptr, i64 }, ptr %local.0, align 8
  %v.24 = extractvalue { ptr, i64 } %v.23, 1
  %v.25 = icmp slt i64 %v.22, %v.24
  br i1 %v.25, label %bb.8, label %bb.9
bb.8:
  %v.26 = load i64, ptr %local.3, align 8
  store i64 %v.26, ptr %local.4, align 8
  %v.27 = load { ptr, i64 }, ptr %local.0, align 8
  %v.28 = load i64, ptr %local.4, align 8
  %tmp.8 = extractvalue { ptr, i64 } %v.27, 0
  %tmp.9 = extractvalue { ptr, i64 } %v.27, 1
  %tmp.10 = icmp uge i64 %v.28, %tmp.9
  br i1 %tmp.10, label %zil.trap, label %checked.2
checked.2:
  %v.29 = getelementptr i8, ptr %tmp.8, i64 %v.28
  %v.30 = load { ptr, i64 }, ptr %local.0, align 8
  %v.31 = load i64, ptr %local.4, align 8
  %tmp.11 = extractvalue { ptr, i64 } %v.30, 0
  %tmp.12 = extractvalue { ptr, i64 } %v.30, 1
  %tmp.13 = icmp uge i64 %v.31, %tmp.12
  br i1 %tmp.13, label %zil.trap, label %checked.3
checked.3:
  %v.32 = getelementptr i8, ptr %tmp.11, i64 %v.31
  %v.33 = getelementptr i8, ptr %v.32, i64 0
  %v.34 = load i8, ptr %v.33, align 1
  %v.35 = add i8 0, 1
  %tmp.14 = call { i8, i1 } @llvm.uadd.with.overflow.i8(i8 %v.34, i8 %v.35)
  %v.36 = extractvalue { i8, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i8, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.4
checked.4:
  store i8 %v.36, ptr %v.29, align 1
  %v.37 = load i64, ptr %local.3, align 8
  %v.38 = add i64 0, 1
  %tmp.16 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.37, i64 %v.38)
  %v.39 = extractvalue { i64, i1 } %tmp.16, 0
  %tmp.17 = extractvalue { i64, i1 } %tmp.16, 1
  br i1 %tmp.17, label %zil.trap, label %checked.5
checked.5:
  store i64 %v.39, ptr %local.3, align 8
  br label %bb.7
bb.9:
  %v.40 = load ptr, ptr %local.2, align 8
  %tmp.18 = getelementptr { ptr, i64, i64 }, ptr %v.40, i32 0, i32 1
  %v.41 = load i64, ptr %tmp.18, align 8
  %tmp.19 = icmp slt i64 %v.41, -2147483648
  %tmp.20 = icmp sgt i64 %v.41, 2147483647
  %tmp.21 = or i1 %tmp.19, %tmp.20
  br i1 %tmp.21, label %zil.trap, label %checked.6
checked.6:
  %v.42 = trunc i64 %v.41 to i32
  ret i32 %v.42
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.12(ptr %arg.0, { ptr, i64 } %arg.1) {
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
  %v.3 = call i32 @zil.fn.11(ptr %v.0, { ptr, i64 } %v.2)
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.13(%zil_env_1 %env, { ptr, i64 } %arg.0, { ptr, i64 } %arg.1, { ptr, i64 } %arg.2, { ptr, i64 } %arg.3, { ptr, i64 } %arg.4, { ptr, i64 } %arg.5) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  %local.2 = alloca { ptr, i64 }, align 8
  %local.3 = alloca { ptr, i64 }, align 8
  %local.4 = alloca { ptr, i64 }, align 8
  %local.5 = alloca { ptr, i64 }, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  store { ptr, i64 } %arg.1, ptr %local.1, align 8
  store { ptr, i64 } %arg.2, ptr %local.2, align 8
  store { ptr, i64 } %arg.3, ptr %local.3, align 8
  store { ptr, i64 } %arg.4, ptr %local.4, align 8
  store { ptr, i64 } %arg.5, ptr %local.5, align 8
  br label %bb.0
bb.0:
  %v.0 = load { ptr, i64 }, ptr %local.0, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.0, 0
  %v.1 = insertvalue { ptr, i64 } %v.0, ptr %tmp.0, 0
  %v.2 = call i64 @zil.fn.7({ ptr, i64 } %v.1)
  %v.3 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.1 = extractvalue { ptr, i64 } %v.3, 0
  %v.4 = insertvalue { ptr, i64 } %v.3, ptr %tmp.1, 0
  %v.5 = call i64 @zil.fn.7({ ptr, i64 } %v.4)
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.2, i64 %v.5)
  %v.6 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.7 = load { ptr, i64 }, ptr %local.2, align 8
  %tmp.4 = extractvalue { ptr, i64 } %v.7, 0
  %v.8 = insertvalue { ptr, i64 } %v.7, ptr %tmp.4, 0
  %v.9 = call i64 @zil.fn.7({ ptr, i64 } %v.8)
  %tmp.5 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.6, i64 %v.9)
  %v.10 = extractvalue { i64, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i64, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  %v.11 = load { ptr, i64 }, ptr %local.3, align 8
  %tmp.7 = extractvalue { ptr, i64 } %v.11, 0
  %v.12 = insertvalue { ptr, i64 } %v.11, ptr %tmp.7, 0
  %v.13 = call i64 @zil.fn.7({ ptr, i64 } %v.12)
  %tmp.8 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.10, i64 %v.13)
  %v.14 = extractvalue { i64, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i64, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  %v.15 = load { ptr, i64 }, ptr %local.4, align 8
  %tmp.10 = extractvalue { ptr, i64 } %v.15, 0
  %v.16 = insertvalue { ptr, i64 } %v.15, ptr %tmp.10, 0
  %v.17 = call i64 @zil.fn.7({ ptr, i64 } %v.16)
  %tmp.11 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.14, i64 %v.17)
  %v.18 = extractvalue { i64, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i64, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.3
checked.3:
  %v.19 = load { ptr, i64 }, ptr %local.5, align 8
  %tmp.13 = extractvalue { ptr, i64 } %v.19, 0
  %v.20 = insertvalue { ptr, i64 } %v.19, ptr %tmp.13, 0
  %v.21 = call i64 @zil.fn.7({ ptr, i64 } %v.20)
  %tmp.14 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.18, i64 %v.21)
  %v.22 = extractvalue { i64, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i64, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.4
checked.4:
  %v.23 = load { ptr, i64 }, ptr %local.0, align 8
  %v.24 = extractvalue { ptr, i64 } %v.23, 1
  %tmp.16 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.22, i64 %v.24)
  %v.25 = extractvalue { i64, i1 } %tmp.16, 0
  %tmp.17 = extractvalue { i64, i1 } %tmp.16, 1
  br i1 %tmp.17, label %zil.trap, label %checked.5
checked.5:
  %v.26 = load { ptr, i64 }, ptr %local.1, align 8
  %v.27 = extractvalue { ptr, i64 } %v.26, 1
  %tmp.18 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.25, i64 %v.27)
  %v.28 = extractvalue { i64, i1 } %tmp.18, 0
  %tmp.19 = extractvalue { i64, i1 } %tmp.18, 1
  br i1 %tmp.19, label %zil.trap, label %checked.6
checked.6:
  %v.29 = load { ptr, i64 }, ptr %local.2, align 8
  %v.30 = extractvalue { ptr, i64 } %v.29, 1
  %tmp.20 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.28, i64 %v.30)
  %v.31 = extractvalue { i64, i1 } %tmp.20, 0
  %tmp.21 = extractvalue { i64, i1 } %tmp.20, 1
  br i1 %tmp.21, label %zil.trap, label %checked.7
checked.7:
  %v.32 = load { ptr, i64 }, ptr %local.3, align 8
  %v.33 = extractvalue { ptr, i64 } %v.32, 1
  %tmp.22 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.31, i64 %v.33)
  %v.34 = extractvalue { i64, i1 } %tmp.22, 0
  %tmp.23 = extractvalue { i64, i1 } %tmp.22, 1
  br i1 %tmp.23, label %zil.trap, label %checked.8
checked.8:
  %v.35 = load { ptr, i64 }, ptr %local.4, align 8
  %v.36 = extractvalue { ptr, i64 } %v.35, 1
  %tmp.24 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.34, i64 %v.36)
  %v.37 = extractvalue { i64, i1 } %tmp.24, 0
  %tmp.25 = extractvalue { i64, i1 } %tmp.24, 1
  br i1 %tmp.25, label %zil.trap, label %checked.9
checked.9:
  %v.38 = load { ptr, i64 }, ptr %local.5, align 8
  %v.39 = extractvalue { ptr, i64 } %v.38, 1
  %tmp.26 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.37, i64 %v.39)
  %v.40 = extractvalue { i64, i1 } %tmp.26, 0
  %tmp.27 = extractvalue { i64, i1 } %tmp.26, 1
  br i1 %tmp.27, label %zil.trap, label %checked.10
checked.10:
  ret i64 %v.40
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.14(%zil_env_2 %env, { ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { ptr, i64 }, ptr %local.0, align 8
  %v.1 = add i32 0, 1
  %tmp.0 = extractvalue { ptr, i64 } %v.0, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.0, 1
  %tmp.2 = sext i32 %v.1 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.2 = getelementptr { i8, i64 }, ptr %tmp.0, i64 %tmp.2
  %v.3 = getelementptr { i8, i64 }, ptr %v.2, i32 0, i32 1
  %v.4 = add i64 0, 41
  store i64 %v.4, ptr %v.3, align 8
  %v.5 = load { ptr, i64 }, ptr %local.0, align 8
  %v.6 = add i32 0, 1
  %tmp.4 = extractvalue { ptr, i64 } %v.5, 0
  %tmp.5 = extractvalue { ptr, i64 } %v.5, 1
  %tmp.6 = sext i32 %v.6 to i64
  %tmp.7 = icmp uge i64 %tmp.6, %tmp.5
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.7 = getelementptr { i8, i64 }, ptr %tmp.4, i64 %tmp.6
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = load { i8, i64 }, ptr %v.8, align 8
%v.10 = extractvalue { i8, i64 } %v.9, 1
  ret i64 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.15(%zil_env_3 %env, { ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %index.slot.12 = alloca [2 x i32], align 4
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { ptr, i64 }, ptr %local.0, align 8
  %v.1 = add i32 0, 1
  %tmp.0 = extractvalue { ptr, i64 } %v.0, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.0, 1
  %tmp.2 = sext i32 %v.1 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.2 = getelementptr [2 x i32], ptr %tmp.0, i64 %tmp.2
  %v.3 = add i32 0, 0
  %tmp.4 = sext i32 %v.3 to i64
  %tmp.5 = icmp ult i64 %tmp.4, 2
  br i1 %tmp.5, label %address.ok.1, label %zil.trap
address.ok.1:
  %v.4 = getelementptr [2 x i32], ptr %v.2, i64 0, i64 %tmp.4
  %v.5 = add i32 0, 41
  store i32 %v.5, ptr %v.4, align 4
  %v.6 = load { ptr, i64 }, ptr %local.0, align 8
  %v.7 = add i32 0, 1
  %tmp.6 = extractvalue { ptr, i64 } %v.6, 0
  %tmp.7 = extractvalue { ptr, i64 } %v.6, 1
  %tmp.8 = sext i32 %v.7 to i64
  %tmp.9 = icmp uge i64 %tmp.8, %tmp.7
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  %v.8 = getelementptr [2 x i32], ptr %tmp.6, i64 %tmp.8
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = load [2 x i32], ptr %v.9, align 4
  %v.11 = add i32 0, 0
  %tmp.10 = sext i32 %v.11 to i64
  %tmp.11 = icmp ult i64 %tmp.10, 2
  br i1 %tmp.11, label %index.ok.3, label %zil.trap
index.ok.3:
  store [2 x i32] %v.10, ptr %index.slot.12, align 4
  %tmp.12 = getelementptr [2 x i32], ptr %index.slot.12, i64 0, i64 %tmp.10
  %v.12 = load i32, ptr %tmp.12, align 4
  ret i32 %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.16(%zil_env_4 %env, { ptr, i64 } %arg.0) {
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
  %v.2 = getelementptr {}, ptr %tmp.0, i64 %tmp.2
  %v.4 = load { ptr, i64 }, ptr %local.0, align 8
  %v.5 = extractvalue { ptr, i64 } %v.4, 1
  ret i64 %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.17(%zil_env_5 %env, { ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { ptr, i64 }, ptr %local.0, align 8
  %v.1 = extractvalue { ptr, i64 } %v.0, 1
  ret i64 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.18(%zil_env_6 %env, { ptr, i64 } %arg.0) {
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
  %v.2 = getelementptr i32, ptr %tmp.0, i64 %tmp.2
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = load i32, ptr %v.3, align 4
  %v.5 = load { ptr, i64 }, ptr %local.0, align 8
  %v.6 = extractvalue { ptr, i64 } %v.5, 1
  %tmp.4 = icmp slt i64 %v.6, -2147483648
  %tmp.5 = icmp sgt i64 %v.6, 2147483647
  %tmp.6 = or i1 %tmp.4, %tmp.5
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  %v.7 = trunc i64 %v.6 to i32
  %tmp.7 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.19(%zil_env_7 %env, { ptr, i64 } %arg.0) {
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
  %v.6 = load { ptr, i64 }, ptr %local.0, align 8
  %v.7 = add i32 0, 1
  %tmp.4 = extractvalue { ptr, i64 } %v.6, 0
  %tmp.5 = extractvalue { ptr, i64 } %v.6, 1
  %tmp.6 = sext i32 %v.7 to i64
  %tmp.7 = icmp uge i64 %tmp.6, %tmp.5
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.8 = getelementptr i8, ptr %tmp.4, i64 %tmp.6
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = load i8, ptr %v.9, align 1
  %v.11 = zext i8 %v.10 to i32
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.20(ptr %arg.0, { ptr, i64 } %arg.1) {
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
  %tmp.1 = load %zil_env_7, ptr %v.0, align 1
  %v.3 = call i32 @zil.fn.19(%zil_env_7 %tmp.1, { ptr, i64 } %v.2)
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.21(%zil_env_8 %env, { ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca i32, align 4
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load { ptr, i64 }, ptr %local.0, align 8
  %v.2 = load i32, ptr %local.1, align 4
  %tmp.0 = extractvalue { ptr, i64 } %v.1, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.1, 1
  %tmp.2 = sext i32 %v.2 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.3 = getelementptr i8, ptr %tmp.0, i64 %tmp.2
  %v.4 = load { ptr, i64 }, ptr %local.0, align 8
  %v.5 = load i32, ptr %local.1, align 4
  %tmp.4 = extractvalue { ptr, i64 } %v.4, 0
  %tmp.5 = extractvalue { ptr, i64 } %v.4, 1
  %tmp.6 = sext i32 %v.5 to i64
  %tmp.7 = icmp uge i64 %tmp.6, %tmp.5
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.6 = getelementptr i8, ptr %tmp.4, i64 %tmp.6
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = load i8, ptr %v.7, align 1
  %v.9 = load { ptr, i64 }, ptr %local.0, align 8
  %v.10 = add i32 0, 1
  %tmp.8 = extractvalue { ptr, i64 } %v.9, 0
  %tmp.9 = extractvalue { ptr, i64 } %v.9, 1
  %tmp.10 = sext i32 %v.10 to i64
  %tmp.11 = icmp uge i64 %tmp.10, %tmp.9
  br i1 %tmp.11, label %zil.trap, label %checked.2
checked.2:
  %v.11 = getelementptr i8, ptr %tmp.8, i64 %tmp.10
  %v.12 = getelementptr i8, ptr %v.11, i64 0
  %v.13 = load i8, ptr %v.12, align 1
  %tmp.12 = call { i8, i1 } @llvm.uadd.with.overflow.i8(i8 %v.8, i8 %v.13)
  %v.14 = extractvalue { i8, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i8, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.3
checked.3:
  store i8 %v.14, ptr %v.3, align 1
  %v.15 = load { ptr, i64 }, ptr %local.0, align 8
  %v.16 = add i32 0, 0
  %tmp.14 = extractvalue { ptr, i64 } %v.15, 0
  %tmp.15 = extractvalue { ptr, i64 } %v.15, 1
  %tmp.16 = sext i32 %v.16 to i64
  %tmp.17 = icmp uge i64 %tmp.16, %tmp.15
  br i1 %tmp.17, label %zil.trap, label %checked.4
checked.4:
  %v.17 = getelementptr i8, ptr %tmp.14, i64 %tmp.16
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  %v.19 = load i8, ptr %v.18, align 1
  %v.20 = zext i8 %v.19 to i32
  ret i32 %v.20
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.22(%zil_env_9 %env) {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 1
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.23(ptr %arg.0, { ptr, i64 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { ptr, i64 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_9, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.22(%zil_env_9 %tmp.0)
  %v.2 = load { ptr, i64 }, ptr %local.1, align 8
  %v.3 = add i32 0, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.2, 0
  %tmp.2 = extractvalue { ptr, i64 } %v.2, 1
  %tmp.3 = sext i32 %v.3 to i64
  %tmp.4 = icmp uge i64 %tmp.3, %tmp.2
  br i1 %tmp.4, label %zil.trap, label %checked.0
checked.0:
  %v.4 = getelementptr i8, ptr %tmp.1, i64 %tmp.3
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = load i8, ptr %v.5, align 1
  %v.7 = zext i8 %v.6 to i32
  %tmp.5 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.24(ptr %env, { ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca ptr, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  %local.3 = alloca i32, align 4
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_10, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.1, align 8
  store i1 false, ptr %resource.live.2, align 1
  %tmp.0 = getelementptr %zil_env_10, ptr %env, i32 0, i32 1
  store i1 false, ptr %tmp.0, align 1
  %tmp.1 = getelementptr %zil_env_10, ptr %env, i32 0, i32 0
  %v.1 = load { ptr, i64, i64 }, ptr %tmp.1, align 8
  store { ptr, i64, i64 } %v.1, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = load { ptr, i64, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.6({ ptr, i64, i64 } %v.2)
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.bytes.drop(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  %v.4 = add i32 0, 0
  store i32 %v.4, ptr %local.3, align 4
  %v.5 = load { ptr, i64 }, ptr %local.0, align 8
  %v.6 = load i32, ptr %local.3, align 4
  %tmp.3 = extractvalue { ptr, i64 } %v.5, 0
  %tmp.4 = extractvalue { ptr, i64 } %v.5, 1
  %tmp.5 = sext i32 %v.6 to i64
  %tmp.6 = icmp uge i64 %tmp.5, %tmp.4
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  %v.7 = getelementptr i8, ptr %tmp.3, i64 %tmp.5
  %v.8 = load { ptr, i64 }, ptr %local.0, align 8
  %v.9 = load i32, ptr %local.3, align 4
  %tmp.7 = extractvalue { ptr, i64 } %v.8, 0
  %tmp.8 = extractvalue { ptr, i64 } %v.8, 1
  %tmp.9 = sext i32 %v.9 to i64
  %tmp.10 = icmp uge i64 %tmp.9, %tmp.8
  br i1 %tmp.10, label %zil.trap, label %checked.2
checked.2:
  %v.10 = getelementptr i8, ptr %tmp.7, i64 %tmp.9
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = load i8, ptr %v.11, align 1
  %v.13 = add i8 0, 1
  %tmp.11 = call { i8, i1 } @llvm.uadd.with.overflow.i8(i8 %v.12, i8 %v.13)
  %v.14 = extractvalue { i8, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i8, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.3
checked.3:
  store i8 %v.14, ptr %v.7, align 1
  %v.15 = load { ptr, i64 }, ptr %local.0, align 8
  %v.16 = add i32 0, 0
  %tmp.13 = extractvalue { ptr, i64 } %v.15, 0
  %tmp.14 = extractvalue { ptr, i64 } %v.15, 1
  %tmp.15 = sext i32 %v.16 to i64
  %tmp.16 = icmp uge i64 %tmp.15, %tmp.14
  br i1 %tmp.16, label %zil.trap, label %checked.4
checked.4:
  %v.17 = getelementptr i8, ptr %tmp.13, i64 %tmp.15
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  %v.19 = load i8, ptr %v.18, align 1
  %v.20 = zext i8 %v.19 to i32
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.20
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.25(%zil_env_10 %arg.0, { ptr, i64 } %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_10, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_10, align 8
  %closure.once.3 = alloca %zil_env_10, align 8
  store %zil_env_10 %arg.0, ptr %local.0, align 8
  store { ptr, i64 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = load %zil_env_10, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_10 %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.1 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.1, 0
  %v.2 = insertvalue { ptr, i64 } %v.1, ptr %tmp.0, 0
  %v.3 = load %zil_env_10, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_10 %v.3, ptr %closure.once.3, align 8
  %v.4 = call i32 @zil.fn.24(ptr %closure.once.3, { ptr, i64 } %v.2)
  call void @zil.closure.drop.10(ptr %closure.once.3)
  %tmp.1 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.1, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.10(ptr %local.2)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.2 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.2, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.10(ptr %local.0)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
declare void @ExitProcess(i32)
define void @zil.pe.entry() {
entry:
  %code = call i32 @main()
  call void @ExitProcess(i32 %code)
  unreachable
}
