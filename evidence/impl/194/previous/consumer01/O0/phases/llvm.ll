; ZIL verified LLVM module
; verified-fingerprint: 8fe315de0b0084cb1e72297cc5480665b8dc7e04108f55b01da901d03c67b038
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32 }, { ptr, i64, i64 }, i1, i1 }
%zil_env_1 = type {  }
%zil_env_2 = type {  }
%zil_env_3 = type {  }
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
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)
declare { i8, i1 } @llvm.uadd.with.overflow.i8(i8, i8)

declare i32 @getchar() nonlazybind
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
  %local.7 = alloca { i32, i32 }, align 4
  %local.8 = alloca i64, align 8
  %local.9 = alloca i1, align 1
  %local.10 = alloca { i32, { i64, i1 } }, align 8
  %local.11 = alloca i1, align 1
  %local.12 = alloca i1, align 1
  %index.slot.22 = alloca [2 x i8], align 1
  %index.slot.94 = alloca [2 x i8], align 1
  %index.slot.104 = alloca [2 x i8], align 1
  br label %bb.0
bb.0:
  %v.0 = call i32 @zil.fn.7()
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
  %v.12 = add i32 0, 55
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
  %v.31 = add i32 0, 0
  store i32 %v.31, ptr %local.6, align 4
  %v.32 = load i32, ptr %local.5, align 4
  %v.33 = add i32 0, 2
  %tmp.6 = icmp eq i32 %v.33, 0
  %tmp.7 = icmp eq i32 %v.32, -2147483648
  %tmp.8 = icmp eq i32 %v.33, -1
  %tmp.9 = and i1 %tmp.7, %tmp.8
  %tmp.10 = or i1 %tmp.6, %tmp.9
  br i1 %tmp.10, label %zil.trap, label %checked.2
checked.2:
  %v.34 = srem i32 %v.32, %v.33
  %v.35 = add i32 0, 0
  %v.36 = icmp eq i32 %v.34, %v.35
  br i1 %v.36, label %bb.13, label %bb.14
bb.12:
  %v.110 = add i32 0, 0
  %tmp.11 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.11, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.110
bb.13:
  %v.37 = getelementptr i8, ptr %local.4, i64 0
  %v.38 = getelementptr i8, ptr %v.37, i64 0
  %v.39 = getelementptr i8, ptr %v.38, i64 0
  %v.40 = load i32, ptr %local.5, align 4
  %v.41 = getelementptr i8, ptr %local.3, i64 0
  %tmp.12 = icmp ugt i64 0, 2
  %tmp.13 = icmp ugt i64 2, 2
  %tmp.14 = or i1 %tmp.12, %tmp.13
  br i1 %tmp.14, label %zil.trap, label %checked.4
checked.4:
  %tmp.15 = getelementptr i8, ptr %v.41, i64 0
  %tmp.16 = sub i64 2, 0
  %tmp.17 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.15, 0
  %v.42 = insertvalue { ptr, i64 } %tmp.17, i64 %tmp.16, 1
  %tmp.18 = extractvalue { ptr, i64 } %v.42, 0
  %v.43 = insertvalue { ptr, i64 } %v.42, ptr %tmp.18, 0
  %tmp.19 = extractvalue { ptr, i64 } %v.43, 0
  %v.44 = insertvalue { ptr, i64 } %v.43, ptr %tmp.19, 0
  %v.45 = call { i32, i32 } @zil.fn.9(ptr %v.39, i32 %v.40, { ptr, i64 } %v.44)
  store { i32, i32 } %v.45, ptr %local.7, align 4
  %v.46 = load { i32, i32 }, ptr %local.7, align 4
%v.47 = extractvalue { i32, i32 } %v.46, 1
  %v.48 = load i32, ptr %local.5, align 4
  %v.49 = icmp ne i32 %v.47, %v.48
  br i1 %v.49, label %bb.15, label %bb.16
bb.14:
  %v.53 = getelementptr i8, ptr %local.4, i64 0
  %v.54 = getelementptr i8, ptr %v.53, i64 0
  %v.55 = getelementptr i8, ptr %v.54, i64 0
  %v.56 = load i32, ptr %local.5, align 4
  %v.57 = sext i32 %v.56 to i64
  store i64 %v.57, ptr %local.8, align 8
  %v.58 = or i1 false, true
  store i1 %v.58, ptr %local.9, align 1
  %v.59 = load i64, ptr %local.8, align 8
  %v.60 = load i1, ptr %local.9, align 1
%tmp.20 = insertvalue { i64, i1 } zeroinitializer, i64 %v.59, 0
%v.61 = insertvalue { i64, i1 } %tmp.20, i1 %v.60, 1
  %v.62 = getelementptr i8, ptr %local.3, i64 0
  %tmp.21 = icmp ugt i64 0, 2
  %tmp.22 = icmp ugt i64 2, 2
  %tmp.23 = or i1 %tmp.21, %tmp.22
  br i1 %tmp.23, label %zil.trap, label %checked.5
checked.5:
  %tmp.24 = getelementptr i8, ptr %v.62, i64 0
  %tmp.25 = sub i64 2, 0
  %tmp.26 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.24, 0
  %v.63 = insertvalue { ptr, i64 } %tmp.26, i64 %tmp.25, 1
  %tmp.27 = extractvalue { ptr, i64 } %v.63, 0
  %v.64 = insertvalue { ptr, i64 } %v.63, ptr %tmp.27, 0
  %tmp.28 = extractvalue { ptr, i64 } %v.64, 0
  %v.65 = insertvalue { ptr, i64 } %v.64, ptr %tmp.28, 0
  %v.66 = call { i32, { i64, i1 } } @zil.fn.10(ptr %v.55, { i64, i1 } %v.61, { ptr, i64 } %v.65)
  store { i32, { i64, i1 } } %v.66, ptr %local.10, align 8
  %v.67 = load { i32, { i64, i1 } }, ptr %local.10, align 8
%v.68 = extractvalue { i32, { i64, i1 } } %v.67, 1
%v.69 = extractvalue { i64, i1 } %v.68, 0
  %v.70 = load i32, ptr %local.5, align 4
  %v.71 = sext i32 %v.70 to i64
  %v.72 = icmp ne i64 %v.69, %v.71
  br i1 %v.72, label %bb.18, label %bb.19
bb.15:
  %v.50 = add i32 0, 8
  %tmp.29 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.29, label %closure.drop.6, label %closure.done.6
closure.drop.6:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.6
closure.done.6:
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.50
bb.16:
  br label %bb.17
bb.17:
  %v.51 = load { i32, i32 }, ptr %local.7, align 4
%v.52 = extractvalue { i32, i32 } %v.51, 0
  store i32 %v.52, ptr %local.6, align 4
  br label %bb.24
bb.18:
  %v.73 = or i1 false, true
  store i1 %v.73, ptr %local.11, align 1
  br label %bb.20
bb.19:
  %v.74 = load { i32, { i64, i1 } }, ptr %local.10, align 8
%v.75 = extractvalue { i32, { i64, i1 } } %v.74, 1
%v.76 = extractvalue { i64, i1 } %v.75, 1
  %v.77 = xor i1 %v.76, true
  store i1 %v.77, ptr %local.11, align 1
  br label %bb.20
bb.20:
  %v.78 = load i1, ptr %local.11, align 1
  br i1 %v.78, label %bb.21, label %bb.22
bb.21:
  %v.79 = add i32 0, 8
  %tmp.30 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.30, label %closure.drop.7, label %closure.done.7
closure.drop.7:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.7
closure.done.7:
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.79
bb.22:
  br label %bb.23
bb.23:
  %v.80 = load { i32, { i64, i1 } }, ptr %local.10, align 8
%v.81 = extractvalue { i32, { i64, i1 } } %v.80, 0
  store i32 %v.81, ptr %local.6, align 4
  br label %bb.24
bb.24:
  %v.82 = load i32, ptr %local.6, align 4
  %v.83 = add i32 0, 0
  %v.84 = icmp slt i32 %v.82, %v.83
  br i1 %v.84, label %bb.25, label %bb.26
bb.25:
  %v.85 = add i32 0, 1
  %tmp.31 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.31, label %closure.drop.8, label %closure.done.8
closure.drop.8:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.8
closure.done.8:
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.85
bb.26:
  br label %bb.27
bb.27:
  %v.86 = load i32, ptr %local.6, align 4
  %v.87 = load i32, ptr %local.5, align 4
  %v.88 = add i32 0, 1
  %tmp.32 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.87, i32 %v.88)
  %v.89 = extractvalue { i32, i1 } %tmp.32, 0
  %tmp.33 = extractvalue { i32, i1 } %tmp.32, 1
  br i1 %tmp.33, label %zil.trap, label %checked.9
checked.9:
  %v.90 = icmp ne i32 %v.86, %v.89
  br i1 %v.90, label %bb.28, label %bb.29
bb.28:
  %v.91 = or i1 false, true
  store i1 %v.91, ptr %local.12, align 1
  br label %bb.30
bb.29:
  %v.92 = load [2 x i8], ptr %local.3, align 1
  %v.93 = add i32 0, 1
  %tmp.34 = sext i32 %v.93 to i64
  %tmp.35 = icmp ult i64 %tmp.34, 2
  br i1 %tmp.35, label %index.ok.10, label %zil.trap
index.ok.10:
  store [2 x i8] %v.92, ptr %index.slot.94, align 1
  %tmp.36 = getelementptr [2 x i8], ptr %index.slot.94, i64 0, i64 %tmp.34
  %v.94 = load i8, ptr %tmp.36, align 1
  %v.95 = add i32 0, 98
  %v.96 = load i32, ptr %local.5, align 4
  %tmp.37 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.95, i32 %v.96)
  %v.97 = extractvalue { i32, i1 } %tmp.37, 0
  %tmp.38 = extractvalue { i32, i1 } %tmp.37, 1
  br i1 %tmp.38, label %zil.trap, label %checked.11
checked.11:
  %tmp.39 = icmp ugt i32 %v.97, 255
  br i1 %tmp.39, label %zil.trap, label %checked.12
checked.12:
  %v.98 = trunc i32 %v.97 to i8
  %v.99 = icmp ne i8 %v.94, %v.98
  store i1 %v.99, ptr %local.12, align 1
  br label %bb.30
bb.30:
  %v.100 = load i1, ptr %local.12, align 1
  br i1 %v.100, label %bb.31, label %bb.32
bb.31:
  %v.101 = add i32 0, 8
  %tmp.40 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.40, label %closure.drop.13, label %closure.done.13
closure.drop.13:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.13
closure.done.13:
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.101
bb.32:
  br label %bb.33
bb.33:
  %v.102 = load [2 x i8], ptr %local.3, align 1
  %v.103 = add i32 0, 0
  %tmp.41 = sext i32 %v.103 to i64
  %tmp.42 = icmp ult i64 %tmp.41, 2
  br i1 %tmp.42, label %index.ok.14, label %zil.trap
index.ok.14:
  store [2 x i8] %v.102, ptr %index.slot.104, align 1
  %tmp.43 = getelementptr [2 x i8], ptr %index.slot.104, i64 0, i64 %tmp.41
  %v.104 = load i8, ptr %tmp.43, align 1
  %v.105 = zext i8 %v.104 to i32
  call void @zil.fn.5(i32 %v.105)
  %v.107 = load i32, ptr %local.5, align 4
  %v.108 = add i32 0, 1
  %tmp.44 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.107, i32 %v.108)
  %v.109 = extractvalue { i32, i1 } %tmp.44, 0
  %tmp.45 = extractvalue { i32, i1 } %tmp.44, 1
  br i1 %tmp.45, label %zil.trap, label %checked.15
checked.15:
  store i32 %v.109, ptr %local.5, align 4
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
  %v.0 = add i32 0, 90
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

declare i32 @putchar(i32) nonlazybind
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

define { { i32 }, { i32 } } @zil.fn.6({ i32 } %arg.0) {
entry:
  %local.0 = alloca { i32 }, align 4
  store { i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i32 }, ptr %local.0, align 4
  %v.1 = load { i32 }, ptr %local.0, align 4
%tmp.0 = insertvalue { { i32 }, { i32 } } zeroinitializer, { i32 } %v.0, 0
%v.2 = insertvalue { { i32 }, { i32 } } %tmp.0, { i32 } %v.1, 1
  ret { { i32 }, { i32 } } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7() {
entry:
  %local.0 = alloca %zil_env_1, align 1
  %local.1 = alloca %zil_env_2, align 1
  %local.2 = alloca i32, align 4
  %local.3 = alloca i64, align 8
  %local.4 = alloca %zil_env_3, align 1
  %local.5 = alloca i32, align 4
  %local.6 = alloca { i32 }, align 4
  %local.7 = alloca i32, align 4
  %local.8 = alloca { i32 }, align 4
  %local.9 = alloca { { i32 }, { i32 } }, align 4
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_1 zeroinitializer
  store %zil_env_1 %v.0, ptr %local.0, align 1
  %v.1 = freeze %zil_env_2 zeroinitializer
  store %zil_env_2 %v.1, ptr %local.1, align 1
  %v.2 = getelementptr i8, ptr %local.0, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = add i32 0, 2
  %v.6 = add i32 %v.5, 0
  %v.7 = call i32 @zil.fn.13(ptr %v.4, i32 %v.6)
  store i32 %v.7, ptr %local.2, align 4
  %v.8 = getelementptr i8, ptr %local.1, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = add i32 0, 2
  %v.12 = sext i32 %v.11 to i64
  %v.13 = call i64 @zil.fn.14(ptr %v.10, i64 %v.12)
  store i64 %v.13, ptr %local.3, align 8
  %v.14 = freeze %zil_env_3 zeroinitializer
  store %zil_env_3 %v.14, ptr %local.4, align 1
  %v.15 = getelementptr i8, ptr %local.4, i64 0
  %v.16 = getelementptr i8, ptr %v.15, i64 0
  %v.17 = getelementptr i8, ptr %v.16, i64 0
  %v.18 = add i32 0, 3
  store i32 %v.18, ptr %local.5, align 4
  %v.19 = load i32, ptr %local.5, align 4
%v.20 = insertvalue { i32 } zeroinitializer, i32 %v.19, 0
  %v.21 = call { i32 } @zil.fn.16(ptr %v.17, { i32 } %v.20)
  store { i32 } %v.21, ptr %local.6, align 4
  %v.22 = getelementptr i8, ptr %local.4, i64 0
  %v.23 = getelementptr i8, ptr %v.22, i64 0
  %v.24 = getelementptr i8, ptr %v.23, i64 0
  %v.25 = add i32 0, 4
  store i32 %v.25, ptr %local.7, align 4
  %v.26 = load i32, ptr %local.7, align 4
%v.27 = insertvalue { i32 } zeroinitializer, i32 %v.26, 0
  %v.28 = call { i32 } @zil.fn.17(ptr %v.24, { i32 } %v.27)
  store { i32 } %v.28, ptr %local.8, align 4
  %v.29 = load { i32 }, ptr %local.6, align 4
  %v.30 = call { { i32 }, { i32 } } @zil.fn.6({ i32 } %v.29)
  store { { i32 }, { i32 } } %v.30, ptr %local.9, align 4
  %v.31 = load i32, ptr %local.2, align 4
  %v.32 = load i64, ptr %local.3, align 8
  %tmp.0 = icmp slt i64 %v.32, -2147483648
  %tmp.1 = icmp sgt i64 %v.32, 2147483647
  %tmp.2 = or i1 %tmp.0, %tmp.1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.33 = trunc i64 %v.32 to i32
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.31, i32 %v.33)
  %v.34 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.35 = load { { i32 }, { i32 } }, ptr %local.9, align 4
%v.36 = extractvalue { { i32 }, { i32 } } %v.35, 0
%v.37 = extractvalue { i32 } %v.36, 0
  %tmp.5 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.34, i32 %v.37)
  %v.38 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  %v.39 = load { { i32 }, { i32 } }, ptr %local.9, align 4
%v.40 = extractvalue { { i32 }, { i32 } } %v.39, 1
%v.41 = extractvalue { i32 } %v.40, 0
  %tmp.7 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.38, i32 %v.41)
  %v.42 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.3
checked.3:
  %v.43 = load { i32 }, ptr %local.8, align 4
%v.44 = extractvalue { i32 } %v.43, 0
  %tmp.9 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.42, i32 %v.44)
  %v.45 = extractvalue { i32, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i32, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.4
checked.4:
  %v.46 = add i32 0, 17
  %tmp.11 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.45, i32 %v.46)
  %v.47 = extractvalue { i32, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i32, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.5
checked.5:
  ret i32 %v.47
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.8(ptr %env, { ptr, i64 } %arg.0) {
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

define { i32, i32 } @zil.fn.9(ptr %arg.0, i32 %arg.1, { ptr, i64 } %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca { ptr, i64 }, align 8
  %local.3 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  store { ptr, i64 } %arg.2, ptr %local.2, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { ptr, i64 }, ptr %local.2, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.1, 0
  %v.2 = insertvalue { ptr, i64 } %v.1, ptr %tmp.0, 0
  %v.3 = call i32 @zil.fn.8(ptr %v.0, { ptr, i64 } %v.2)
  store i32 %v.3, ptr %local.3, align 4
  %v.4 = load i32, ptr %local.3, align 4
  %v.5 = load i32, ptr %local.1, align 4
%tmp.1 = insertvalue { i32, i32 } zeroinitializer, i32 %v.4, 0
%v.6 = insertvalue { i32, i32 } %tmp.1, i32 %v.5, 1
  ret { i32, i32 } %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i32, { i64, i1 } } @zil.fn.10(ptr %arg.0, { i64, i1 } %arg.1, { ptr, i64 } %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i64, i1 }, align 8
  %local.2 = alloca { ptr, i64 }, align 8
  %local.3 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { i64, i1 } %arg.1, ptr %local.1, align 8
  store { ptr, i64 } %arg.2, ptr %local.2, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { ptr, i64 }, ptr %local.2, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.1, 0
  %v.2 = insertvalue { ptr, i64 } %v.1, ptr %tmp.0, 0
  %v.3 = call i32 @zil.fn.8(ptr %v.0, { ptr, i64 } %v.2)
  store i32 %v.3, ptr %local.3, align 4
  %v.4 = load i32, ptr %local.3, align 4
  %v.5 = load { i64, i1 }, ptr %local.1, align 8
%tmp.1 = insertvalue { i32, { i64, i1 } } zeroinitializer, i32 %v.4, 0
%v.6 = insertvalue { i32, { i64, i1 } } %tmp.1, { i64, i1 } %v.5, 1
  ret { i32, { i64, i1 } } %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.11(%zil_env_1 %env, i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.0, i32 %v.1)
  %v.2 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.12(%zil_env_2 %env, i64 %arg.0) {
entry:
  %local.0 = alloca i64, align 8
  store i64 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load i64, ptr %local.0, align 8
  %v.1 = add i64 0, 2
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.0, i64 %v.1)
  %v.2 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i64 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.13(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %local.1, align 4
  %tmp.0 = load %zil_env_1, ptr %v.0, align 1
  %v.2 = call i32 @zil.fn.11(%zil_env_1 %tmp.0, i32 %v.1)
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.14(ptr %arg.0, i64 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i64, ptr %local.1, align 8
  %tmp.0 = load %zil_env_2, ptr %v.0, align 1
  %v.2 = call i64 @zil.fn.12(%zil_env_2 %tmp.0, i64 %v.1)
  ret i64 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.15(%zil_env_3 %env) {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 1
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i32 } @zil.fn.16(ptr %arg.0, { i32 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { i32 } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_3, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.15(%zil_env_3 %tmp.0)
  %v.2 = load { i32 }, ptr %local.1, align 4
  ret { i32 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i32 } @zil.fn.17(ptr %arg.0, { i32 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { i32 } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_3, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.15(%zil_env_3 %tmp.0)
  %v.2 = load { i32 }, ptr %local.1, align 4
  ret { i32 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
