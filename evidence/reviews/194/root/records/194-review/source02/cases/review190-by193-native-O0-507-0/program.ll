; ZIL verified LLVM module
; verified-fingerprint: 9206afc54737d3c0878c9f03054d5b352adc1f979d20386edc3d2478b32e09e4
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32 }, i32, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  call void @zil.fn.3(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar() nonlazybind
declare i32 @putchar(i32) nonlazybind
define void @zil.fn.2(i32 %arg.0) {
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

define void @zil.fn.3(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.2(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_0, align 4
  %local.5 = alloca i32, align 4
  %local.6 = alloca i32, align 4
  %local.7 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call i32 @getchar()
  store i32 %v.1, ptr %local.0, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = add i32 0, 48
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.4, ptr %local.0, align 4
  %v.5 = add i32 0, 72
  store i32 %v.5, ptr %local.1, align 4
  %v.6 = load i32, ptr %local.1, align 4
%v.7 = insertvalue { i32 } zeroinitializer, i32 %v.6, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.7, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.8 = add i32 0, 0
  store i32 %v.8, ptr %local.3, align 4
  %v.9 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.10 = load i32, ptr %local.3, align 4
  %tmp.2 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.9, 0
  %tmp.3 = insertvalue %zil_env_0 %tmp.2, i32 %v.10, 1
  %v.11 = insertvalue %zil_env_0 %tmp.3, i1 true, 2
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_0 %v.11, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.12 = getelementptr i8, ptr %local.4, i64 0
  %v.13 = getelementptr i8, ptr %v.12, i64 0
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = load i32, ptr %local.0, align 4
  %v.16 = call i32 @zil.fn.7(ptr %v.14, i32 %v.15)
  store i32 %v.16, ptr %local.5, align 4
  %v.17 = load i32, ptr %local.0, align 4
  %v.18 = load i32, ptr %local.0, align 4
  %v.19 = add i32 0, 1
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.18, i32 %v.19)
  %v.20 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  %tmp.6 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.17, i32 %v.20)
  %v.21 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.21, ptr %local.6, align 4
  %v.22 = add i32 0, 90
  %v.23 = call i32 @zil.fn.5(ptr %local.4, i32 %v.22)
  store i32 %v.23, ptr %local.7, align 4
  %v.24 = load i32, ptr %local.5, align 4
  %v.25 = load i32, ptr %local.6, align 4
  %v.26 = load i32, ptr %local.6, align 4
  %v.27 = add i32 0, 1
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.26, i32 %v.27)
  %v.28 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  %tmp.10 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.25, i32 %v.28)
  %v.29 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.4
checked.4:
  %v.30 = add i32 0, 2
  %tmp.12 = icmp eq i32 %v.30, 0
  %tmp.13 = icmp eq i32 %v.29, -2147483648
  %tmp.14 = icmp eq i32 %v.30, -1
  %tmp.15 = and i1 %tmp.13, %tmp.14
  %tmp.16 = or i1 %tmp.12, %tmp.15
  br i1 %tmp.16, label %zil.trap, label %checked.5
checked.5:
  %v.31 = sdiv i32 %v.29, %v.30
  %tmp.17 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.24, i32 %v.31)
  %v.32 = extractvalue { i32, i1 } %tmp.17, 0
  %tmp.18 = extractvalue { i32, i1 } %tmp.17, 1
  br i1 %tmp.18, label %zil.trap, label %checked.6
checked.6:
  %v.33 = load i32, ptr %local.7, align 4
  %tmp.19 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.32, i32 %v.33)
  %v.34 = extractvalue { i32, i1 } %tmp.19, 0
  %tmp.20 = extractvalue { i32, i1 } %tmp.19, 1
  br i1 %tmp.20, label %zil.trap, label %checked.7
checked.7:
  %v.35 = load i32, ptr %local.6, align 4
  %tmp.21 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.34, i32 %v.35)
  %v.36 = extractvalue { i32, i1 } %tmp.21, 0
  %tmp.22 = extractvalue { i32, i1 } %tmp.21, 1
  br i1 %tmp.22, label %zil.trap, label %checked.8
checked.8:
  %v.37 = add i32 0, 1
  %tmp.23 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.36, i32 %v.37)
  %v.38 = extractvalue { i32, i1 } %tmp.23, 0
  %tmp.24 = extractvalue { i32, i1 } %tmp.23, 1
  br i1 %tmp.24, label %zil.trap, label %checked.9
checked.9:
  %tmp.25 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.25, label %closure.drop.10, label %closure.done.10
closure.drop.10:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.10
closure.done.10:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.26 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.26, label %resource.drop.11, label %resource.done.11
resource.drop.11:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.3(ptr %local.2)
  br label %resource.done.11
resource.done.11:
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.38
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(ptr %env, i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.1, align 8
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %v.1 = load i32, ptr %tmp.0, align 4
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load ptr, ptr %local.1, align 8
  %tmp.1 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.1, align 4
  %v.4 = load i32, ptr %local.2, align 4
  %v.5 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.6, ptr %local.2, align 4
  %tmp.4 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store i32 %v.6, ptr %tmp.4, align 4
  %v.7 = load i32, ptr %local.0, align 4
  call void @zil.fn.2(i32 %v.7)
  %v.9 = load i32, ptr %local.2, align 4
  ret i32 %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6(ptr %arg.0, i32 %arg.1, i32 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  %local.5 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  store i32 %arg.2, ptr %local.2, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.3, align 4
  %v.1 = add i32 0, 0
  %v.2 = load i32, ptr %local.1, align 4
  store i32 %v.1, ptr %local.4, align 4
  br label %bb.1
bb.1:
  %v.3 = load i32, ptr %local.4, align 4
  %v.4 = icmp slt i32 %v.3, %v.2
  br i1 %v.4, label %bb.2, label %bb.4
bb.2:
  store i32 %v.3, ptr %local.5, align 4
  %v.5 = load i32, ptr %local.3, align 4
  %v.6 = load ptr, ptr %local.0, align 8
  %v.7 = load i32, ptr %local.2, align 4
  %v.8 = load i32, ptr %local.5, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.7, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.10 = call i32 @zil.fn.5(ptr %v.6, i32 %v.9)
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.11, ptr %local.3, align 4
  br label %bb.3
bb.3:
  %v.12 = add i32 0, 1
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.13, ptr %local.4, align 4
  br label %bb.1
bb.4:
  %v.14 = load i32, ptr %local.3, align 4
  ret i32 %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = add i32 0, 0
  %v.2 = load i32, ptr %local.1, align 4
  store i32 %v.1, ptr %local.3, align 4
  br label %bb.1
bb.1:
  %v.3 = load i32, ptr %local.3, align 4
  %v.4 = icmp slt i32 %v.3, %v.2
  br i1 %v.4, label %bb.2, label %bb.4
bb.2:
  store i32 %v.3, ptr %local.4, align 4
  %v.5 = load i32, ptr %local.2, align 4
  %v.6 = load ptr, ptr %local.0, align 8
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = load i32, ptr %local.4, align 4
  %v.9 = add i32 0, 65
  %v.10 = call i32 @zil.fn.6(ptr %v.7, i32 %v.8, i32 %v.9)
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.11, ptr %local.2, align 4
  %v.12 = load i32, ptr %local.2, align 4
  %v.13 = load ptr, ptr %local.0, align 8
  %v.14 = add i32 0, 120
  %v.15 = call i32 @zil.fn.5(ptr %v.13, i32 %v.14)
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.12, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.16, ptr %local.2, align 4
  %v.17 = load i32, ptr %local.2, align 4
  %v.18 = load ptr, ptr %local.0, align 8
  %v.19 = getelementptr i8, ptr %v.18, i64 0
  %v.20 = load i32, ptr %local.1, align 4
  %v.21 = load i32, ptr %local.4, align 4
  %tmp.4 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.20, i32 %v.21)
  %v.22 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.23 = add i32 0, 97
  %v.24 = call i32 @zil.fn.6(ptr %v.19, i32 %v.22, i32 %v.23)
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.17, i32 %v.24)
  %v.25 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  store i32 %v.25, ptr %local.2, align 4
  br label %bb.3
bb.3:
  %v.26 = add i32 0, 1
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.26)
  %v.27 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.4
checked.4:
  store i32 %v.27, ptr %local.3, align 4
  br label %bb.1
bb.4:
  %v.28 = load i32, ptr %local.2, align 4
  ret i32 %v.28
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.4()
  ret i32 %entry.result
}
