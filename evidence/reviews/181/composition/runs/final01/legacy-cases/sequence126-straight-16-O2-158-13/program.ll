; ZIL verified LLVM module
; verified-fingerprint: f7eea8d4f36def60931e035e3a886613f06bd8cd2771795b9908bede4d36c54e
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { ptr }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
@zil.return.format = private unnamed_addr constant [6 x i8] c"%lld\0A\00"
declare i32 @printf(ptr, ...)

define i64 @zil.fn.0() {
entry:
  %local.0 = alloca i64, align 8
  %local.1 = alloca %zil_env_0, align 8
  %local.2 = alloca i64, align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 0
  store i64 %v.0, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = insertvalue %zil_env_0 zeroinitializer, ptr %v.2, 0
  store %zil_env_0 %v.3, ptr %local.1, align 8
  %v.4 = getelementptr i8, ptr %local.1, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = call i64 @zil.fn.2(ptr %v.6)
  store i64 %v.7, ptr %local.2, align 8
  %v.8 = load i64, ptr %local.2, align 8
  %v.9 = load i64, ptr %local.0, align 8
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.8, i64 %v.9)
  %v.10 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i64 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.1(ptr %env, i64 %arg.0) {
entry:
  %local.0 = alloca i64, align 8
  %local.1 = alloca ptr, align 8
  store i64 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load ptr, ptr %tmp.0, align 8
  store ptr %v.0, ptr %local.1, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %v.2 = load ptr, ptr %local.1, align 8
  %v.3 = load i64, ptr %v.2, align 8
  %v.4 = load i64, ptr %local.0, align 8
  %tmp.1 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.3, i64 %v.4)
  %v.5 = extractvalue { i64, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i64, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i64 %v.5, ptr %v.1, align 8
  %v.6 = load ptr, ptr %local.1, align 8
  %v.7 = load i64, ptr %v.6, align 8
  ret i64 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.2(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = add i64 0, 1
  %v.2 = call i64 @zil.fn.1(ptr %v.0, i64 %v.1)
  %v.3 = load ptr, ptr %local.0, align 8
  %v.4 = add i64 0, 2
  %v.5 = call i64 @zil.fn.1(ptr %v.3, i64 %v.4)
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.2, i64 %v.5)
  %v.6 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.7 = load ptr, ptr %local.0, align 8
  %v.8 = add i64 0, 3
  %v.9 = call i64 @zil.fn.1(ptr %v.7, i64 %v.8)
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.6, i64 %v.9)
  %v.10 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %v.11 = load ptr, ptr %local.0, align 8
  %v.12 = add i64 0, 4
  %v.13 = call i64 @zil.fn.1(ptr %v.11, i64 %v.12)
  %tmp.4 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.10, i64 %v.13)
  %v.14 = extractvalue { i64, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i64, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.15 = load ptr, ptr %local.0, align 8
  %v.16 = add i64 0, 5
  %v.17 = call i64 @zil.fn.1(ptr %v.15, i64 %v.16)
  %tmp.6 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.14, i64 %v.17)
  %v.18 = extractvalue { i64, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i64, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  %v.19 = load ptr, ptr %local.0, align 8
  %v.20 = add i64 0, 6
  %v.21 = call i64 @zil.fn.1(ptr %v.19, i64 %v.20)
  %tmp.8 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.18, i64 %v.21)
  %v.22 = extractvalue { i64, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i64, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.4
checked.4:
  %v.23 = load ptr, ptr %local.0, align 8
  %v.24 = add i64 0, 7
  %v.25 = call i64 @zil.fn.1(ptr %v.23, i64 %v.24)
  %tmp.10 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.22, i64 %v.25)
  %v.26 = extractvalue { i64, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i64, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.5
checked.5:
  %v.27 = load ptr, ptr %local.0, align 8
  %v.28 = add i64 0, 8
  %v.29 = call i64 @zil.fn.1(ptr %v.27, i64 %v.28)
  %tmp.12 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.26, i64 %v.29)
  %v.30 = extractvalue { i64, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i64, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.6
checked.6:
  %v.31 = load ptr, ptr %local.0, align 8
  %v.32 = add i64 0, 9
  %v.33 = call i64 @zil.fn.1(ptr %v.31, i64 %v.32)
  %tmp.14 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.30, i64 %v.33)
  %v.34 = extractvalue { i64, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i64, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.7
checked.7:
  %v.35 = load ptr, ptr %local.0, align 8
  %v.36 = add i64 0, 10
  %v.37 = call i64 @zil.fn.1(ptr %v.35, i64 %v.36)
  %tmp.16 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.34, i64 %v.37)
  %v.38 = extractvalue { i64, i1 } %tmp.16, 0
  %tmp.17 = extractvalue { i64, i1 } %tmp.16, 1
  br i1 %tmp.17, label %zil.trap, label %checked.8
checked.8:
  %v.39 = load ptr, ptr %local.0, align 8
  %v.40 = add i64 0, 11
  %v.41 = call i64 @zil.fn.1(ptr %v.39, i64 %v.40)
  %tmp.18 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.38, i64 %v.41)
  %v.42 = extractvalue { i64, i1 } %tmp.18, 0
  %tmp.19 = extractvalue { i64, i1 } %tmp.18, 1
  br i1 %tmp.19, label %zil.trap, label %checked.9
checked.9:
  %v.43 = load ptr, ptr %local.0, align 8
  %v.44 = add i64 0, 12
  %v.45 = call i64 @zil.fn.1(ptr %v.43, i64 %v.44)
  %tmp.20 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.42, i64 %v.45)
  %v.46 = extractvalue { i64, i1 } %tmp.20, 0
  %tmp.21 = extractvalue { i64, i1 } %tmp.20, 1
  br i1 %tmp.21, label %zil.trap, label %checked.10
checked.10:
  %v.47 = load ptr, ptr %local.0, align 8
  %v.48 = add i64 0, 13
  %v.49 = call i64 @zil.fn.1(ptr %v.47, i64 %v.48)
  %tmp.22 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.46, i64 %v.49)
  %v.50 = extractvalue { i64, i1 } %tmp.22, 0
  %tmp.23 = extractvalue { i64, i1 } %tmp.22, 1
  br i1 %tmp.23, label %zil.trap, label %checked.11
checked.11:
  %v.51 = load ptr, ptr %local.0, align 8
  %v.52 = add i64 0, 14
  %v.53 = call i64 @zil.fn.1(ptr %v.51, i64 %v.52)
  %tmp.24 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.50, i64 %v.53)
  %v.54 = extractvalue { i64, i1 } %tmp.24, 0
  %tmp.25 = extractvalue { i64, i1 } %tmp.24, 1
  br i1 %tmp.25, label %zil.trap, label %checked.12
checked.12:
  %v.55 = load ptr, ptr %local.0, align 8
  %v.56 = add i64 0, 15
  %v.57 = call i64 @zil.fn.1(ptr %v.55, i64 %v.56)
  %tmp.26 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.54, i64 %v.57)
  %v.58 = extractvalue { i64, i1 } %tmp.26, 0
  %tmp.27 = extractvalue { i64, i1 } %tmp.26, 1
  br i1 %tmp.27, label %zil.trap, label %checked.13
checked.13:
  %v.59 = load ptr, ptr %local.0, align 8
  %v.60 = add i64 0, 16
  %v.61 = call i64 @zil.fn.1(ptr %v.59, i64 %v.60)
  %tmp.28 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.58, i64 %v.61)
  %v.62 = extractvalue { i64, i1 } %tmp.28, 0
  %tmp.29 = extractvalue { i64, i1 } %tmp.28, 1
  br i1 %tmp.29, label %zil.trap, label %checked.14
checked.14:
  ret i64 %v.62
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.wide = call i64 @zil.fn.0()
  %entry.printed = call i32 (ptr, ...) @printf(ptr @zil.return.format, i64 %entry.wide)
  ret i32 0
}
