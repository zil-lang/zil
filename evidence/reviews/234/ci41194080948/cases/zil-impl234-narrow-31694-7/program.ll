; ZIL verified LLVM module
; verified-fingerprint: 2e01df4e03f489e3e9062e0738e750eee80127e2d6dc1b9d9135279db182c82c
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)
declare { i8, i1 } @llvm.usub.with.overflow.i8(i8, i8)
define i32 @zil.fn.0() {
entry:
  %local.0 = alloca [64 x i8], align 1
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i8, align 1
  %local.4 = alloca i1, align 1
  %index.slot.20 = alloca [64 x i8], align 1
  %index.slot.34 = alloca [64 x i8], align 1
  br label %bb.0
bb.0:
  %v.0 = call [64 x i8] @zil.fn.7()
  store [64 x i8] %v.0, ptr %local.0, align 1
  br label %bb.1
bb.1:
  %v.1 = or i1 false, true
  br label %bb.2
bb.2:
  %v.2 = add i32 0, 0
  %v.3 = getelementptr i8, ptr %local.0, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = call i32 @zil.fn.8(i32 %v.2, ptr %v.5)
  store i32 %v.6, ptr %local.1, align 4
  %v.7 = load i32, ptr %local.1, align 4
  %v.8 = add i32 0, 0
  %v.9 = icmp slt i32 %v.7, %v.8
  br i1 %v.9, label %bb.4, label %bb.5
bb.3:
  %v.47 = add i32 0, 0
  ret i32 %v.47
bb.4:
  %v.10 = add i32 0, 1
  ret i32 %v.10
bb.5:
  br label %bb.6
bb.6:
  %v.11 = load i32, ptr %local.1, align 4
  %v.12 = add i32 0, 0
  %v.13 = icmp eq i32 %v.11, %v.12
  br i1 %v.13, label %bb.7, label %bb.8
bb.7:
  br label %bb.3
bb.8:
  br label %bb.9
bb.9:
  %v.14 = add i32 0, 0
  store i32 %v.14, ptr %local.2, align 4
  br label %bb.10
bb.10:
  %v.15 = load i32, ptr %local.2, align 4
  %v.16 = load i32, ptr %local.1, align 4
  %v.17 = icmp slt i32 %v.15, %v.16
  br i1 %v.17, label %bb.11, label %bb.12
bb.11:
  %v.18 = load [64 x i8], ptr %local.0, align 1
  %v.19 = load i32, ptr %local.2, align 4
  %tmp.0 = sext i32 %v.19 to i64
  %tmp.1 = icmp ult i64 %tmp.0, 64
  br i1 %tmp.1, label %index.ok.0, label %zil.trap
index.ok.0:
  store [64 x i8] %v.18, ptr %index.slot.20, align 1
  %tmp.2 = getelementptr [64 x i8], ptr %index.slot.20, i64 0, i64 %tmp.0
  %v.20 = load i8, ptr %tmp.2, align 1
  store i8 %v.20, ptr %local.3, align 1
  %v.21 = load i8, ptr %local.3, align 1
  %v.22 = add i8 0, 97
  %v.23 = icmp uge i8 %v.21, %v.22
  br i1 %v.23, label %bb.14, label %bb.13
bb.12:
  %v.38 = add i32 0, 1
  %v.39 = getelementptr i8, ptr %local.0, i64 0
  %v.40 = getelementptr i8, ptr %v.39, i64 0
  %v.41 = getelementptr i8, ptr %v.40, i64 0
  %v.42 = load i32, ptr %local.1, align 4
  %v.43 = call i32 @zil.fn.9(i32 %v.38, ptr %v.41, i32 %v.42)
  %v.44 = add i32 0, 0
  %v.45 = icmp ne i32 %v.43, %v.44
  br i1 %v.45, label %bb.19, label %bb.20
bb.13:
  %v.24 = or i1 false, false
  store i1 %v.24, ptr %local.4, align 1
  br label %bb.15
bb.14:
  %v.25 = load i8, ptr %local.3, align 1
  %v.26 = add i8 0, 122
  %v.27 = icmp ule i8 %v.25, %v.26
  store i1 %v.27, ptr %local.4, align 1
  br label %bb.15
bb.15:
  %v.28 = load i1, ptr %local.4, align 1
  br i1 %v.28, label %bb.16, label %bb.17
bb.16:
  %v.29 = load i32, ptr %local.2, align 4
  %v.30 = load i8, ptr %local.3, align 1
  %v.31 = add i8 0, 32
  %tmp.3 = call { i8, i1 } @llvm.usub.with.overflow.i8(i8 %v.30, i8 %v.31)
  %v.32 = extractvalue { i8, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i8, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.33 = load [64 x i8], ptr %local.0, align 1
  %tmp.5 = sext i32 %v.29 to i64
  %tmp.6 = icmp ult i64 %tmp.5, 64
  br i1 %tmp.6, label %index.ok.2, label %zil.trap
index.ok.2:
  store [64 x i8] %v.33, ptr %index.slot.34, align 1
  %tmp.7 = getelementptr [64 x i8], ptr %index.slot.34, i64 0, i64 %tmp.5
  store i8 %v.32, ptr %tmp.7, align 1
  %v.34 = load [64 x i8], ptr %index.slot.34, align 1
  store [64 x i8] %v.34, ptr %local.0, align 1
  br label %bb.18
bb.17:
  br label %bb.18
bb.18:
  %v.35 = load i32, ptr %local.2, align 4
  %v.36 = add i32 0, 1
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.35, i32 %v.36)
  %v.37 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  store i32 %v.37, ptr %local.2, align 4
  br label %bb.10
bb.19:
  %v.46 = add i32 0, 2
  ret i32 %v.46
bb.20:
  br label %bb.21
bb.21:
  br label %bb.1
zil.trap:
  call void @llvm.trap()
  unreachable
}
declare i64 @read(i32, ptr, i64) nonlazybind
declare i64 @write(i32, ptr, i64) nonlazybind
declare i32 @pipe(ptr) nonlazybind
declare i32 @close(i32) nonlazybind
declare i32 @dup(i32) nonlazybind
declare i32 @putchar(i32) nonlazybind
define [64 x i8] @zil.fn.7() {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i8 0, 0
  %v.1 = add i8 0, 0
  %v.2 = add i8 0, 0
  %v.3 = add i8 0, 0
  %v.4 = add i8 0, 0
  %v.5 = add i8 0, 0
  %v.6 = add i8 0, 0
  %v.7 = add i8 0, 0
  %v.8 = add i8 0, 0
  %v.9 = add i8 0, 0
  %v.10 = add i8 0, 0
  %v.11 = add i8 0, 0
  %v.12 = add i8 0, 0
  %v.13 = add i8 0, 0
  %v.14 = add i8 0, 0
  %v.15 = add i8 0, 0
  %v.16 = add i8 0, 0
  %v.17 = add i8 0, 0
  %v.18 = add i8 0, 0
  %v.19 = add i8 0, 0
  %v.20 = add i8 0, 0
  %v.21 = add i8 0, 0
  %v.22 = add i8 0, 0
  %v.23 = add i8 0, 0
  %v.24 = add i8 0, 0
  %v.25 = add i8 0, 0
  %v.26 = add i8 0, 0
  %v.27 = add i8 0, 0
  %v.28 = add i8 0, 0
  %v.29 = add i8 0, 0
  %v.30 = add i8 0, 0
  %v.31 = add i8 0, 0
  %v.32 = add i8 0, 0
  %v.33 = add i8 0, 0
  %v.34 = add i8 0, 0
  %v.35 = add i8 0, 0
  %v.36 = add i8 0, 0
  %v.37 = add i8 0, 0
  %v.38 = add i8 0, 0
  %v.39 = add i8 0, 0
  %v.40 = add i8 0, 0
  %v.41 = add i8 0, 0
  %v.42 = add i8 0, 0
  %v.43 = add i8 0, 0
  %v.44 = add i8 0, 0
  %v.45 = add i8 0, 0
  %v.46 = add i8 0, 0
  %v.47 = add i8 0, 0
  %v.48 = add i8 0, 0
  %v.49 = add i8 0, 0
  %v.50 = add i8 0, 0
  %v.51 = add i8 0, 0
  %v.52 = add i8 0, 0
  %v.53 = add i8 0, 0
  %v.54 = add i8 0, 0
  %v.55 = add i8 0, 0
  %v.56 = add i8 0, 0
  %v.57 = add i8 0, 0
  %v.58 = add i8 0, 0
  %v.59 = add i8 0, 0
  %v.60 = add i8 0, 0
  %v.61 = add i8 0, 0
  %v.62 = add i8 0, 0
  %v.63 = add i8 0, 0
  %tmp.0 = insertvalue [64 x i8] zeroinitializer, i8 %v.0, 0
  %tmp.1 = insertvalue [64 x i8] %tmp.0, i8 %v.1, 1
  %tmp.2 = insertvalue [64 x i8] %tmp.1, i8 %v.2, 2
  %tmp.3 = insertvalue [64 x i8] %tmp.2, i8 %v.3, 3
  %tmp.4 = insertvalue [64 x i8] %tmp.3, i8 %v.4, 4
  %tmp.5 = insertvalue [64 x i8] %tmp.4, i8 %v.5, 5
  %tmp.6 = insertvalue [64 x i8] %tmp.5, i8 %v.6, 6
  %tmp.7 = insertvalue [64 x i8] %tmp.6, i8 %v.7, 7
  %tmp.8 = insertvalue [64 x i8] %tmp.7, i8 %v.8, 8
  %tmp.9 = insertvalue [64 x i8] %tmp.8, i8 %v.9, 9
  %tmp.10 = insertvalue [64 x i8] %tmp.9, i8 %v.10, 10
  %tmp.11 = insertvalue [64 x i8] %tmp.10, i8 %v.11, 11
  %tmp.12 = insertvalue [64 x i8] %tmp.11, i8 %v.12, 12
  %tmp.13 = insertvalue [64 x i8] %tmp.12, i8 %v.13, 13
  %tmp.14 = insertvalue [64 x i8] %tmp.13, i8 %v.14, 14
  %tmp.15 = insertvalue [64 x i8] %tmp.14, i8 %v.15, 15
  %tmp.16 = insertvalue [64 x i8] %tmp.15, i8 %v.16, 16
  %tmp.17 = insertvalue [64 x i8] %tmp.16, i8 %v.17, 17
  %tmp.18 = insertvalue [64 x i8] %tmp.17, i8 %v.18, 18
  %tmp.19 = insertvalue [64 x i8] %tmp.18, i8 %v.19, 19
  %tmp.20 = insertvalue [64 x i8] %tmp.19, i8 %v.20, 20
  %tmp.21 = insertvalue [64 x i8] %tmp.20, i8 %v.21, 21
  %tmp.22 = insertvalue [64 x i8] %tmp.21, i8 %v.22, 22
  %tmp.23 = insertvalue [64 x i8] %tmp.22, i8 %v.23, 23
  %tmp.24 = insertvalue [64 x i8] %tmp.23, i8 %v.24, 24
  %tmp.25 = insertvalue [64 x i8] %tmp.24, i8 %v.25, 25
  %tmp.26 = insertvalue [64 x i8] %tmp.25, i8 %v.26, 26
  %tmp.27 = insertvalue [64 x i8] %tmp.26, i8 %v.27, 27
  %tmp.28 = insertvalue [64 x i8] %tmp.27, i8 %v.28, 28
  %tmp.29 = insertvalue [64 x i8] %tmp.28, i8 %v.29, 29
  %tmp.30 = insertvalue [64 x i8] %tmp.29, i8 %v.30, 30
  %tmp.31 = insertvalue [64 x i8] %tmp.30, i8 %v.31, 31
  %tmp.32 = insertvalue [64 x i8] %tmp.31, i8 %v.32, 32
  %tmp.33 = insertvalue [64 x i8] %tmp.32, i8 %v.33, 33
  %tmp.34 = insertvalue [64 x i8] %tmp.33, i8 %v.34, 34
  %tmp.35 = insertvalue [64 x i8] %tmp.34, i8 %v.35, 35
  %tmp.36 = insertvalue [64 x i8] %tmp.35, i8 %v.36, 36
  %tmp.37 = insertvalue [64 x i8] %tmp.36, i8 %v.37, 37
  %tmp.38 = insertvalue [64 x i8] %tmp.37, i8 %v.38, 38
  %tmp.39 = insertvalue [64 x i8] %tmp.38, i8 %v.39, 39
  %tmp.40 = insertvalue [64 x i8] %tmp.39, i8 %v.40, 40
  %tmp.41 = insertvalue [64 x i8] %tmp.40, i8 %v.41, 41
  %tmp.42 = insertvalue [64 x i8] %tmp.41, i8 %v.42, 42
  %tmp.43 = insertvalue [64 x i8] %tmp.42, i8 %v.43, 43
  %tmp.44 = insertvalue [64 x i8] %tmp.43, i8 %v.44, 44
  %tmp.45 = insertvalue [64 x i8] %tmp.44, i8 %v.45, 45
  %tmp.46 = insertvalue [64 x i8] %tmp.45, i8 %v.46, 46
  %tmp.47 = insertvalue [64 x i8] %tmp.46, i8 %v.47, 47
  %tmp.48 = insertvalue [64 x i8] %tmp.47, i8 %v.48, 48
  %tmp.49 = insertvalue [64 x i8] %tmp.48, i8 %v.49, 49
  %tmp.50 = insertvalue [64 x i8] %tmp.49, i8 %v.50, 50
  %tmp.51 = insertvalue [64 x i8] %tmp.50, i8 %v.51, 51
  %tmp.52 = insertvalue [64 x i8] %tmp.51, i8 %v.52, 52
  %tmp.53 = insertvalue [64 x i8] %tmp.52, i8 %v.53, 53
  %tmp.54 = insertvalue [64 x i8] %tmp.53, i8 %v.54, 54
  %tmp.55 = insertvalue [64 x i8] %tmp.54, i8 %v.55, 55
  %tmp.56 = insertvalue [64 x i8] %tmp.55, i8 %v.56, 56
  %tmp.57 = insertvalue [64 x i8] %tmp.56, i8 %v.57, 57
  %tmp.58 = insertvalue [64 x i8] %tmp.57, i8 %v.58, 58
  %tmp.59 = insertvalue [64 x i8] %tmp.58, i8 %v.59, 59
  %tmp.60 = insertvalue [64 x i8] %tmp.59, i8 %v.60, 60
  %tmp.61 = insertvalue [64 x i8] %tmp.60, i8 %v.61, 61
  %tmp.62 = insertvalue [64 x i8] %tmp.61, i8 %v.62, 62
  %v.64 = insertvalue [64 x i8] %tmp.62, i8 %v.63, 63
  ret [64 x i8] %v.64
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.8(i32 %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  store i32 %arg.0, ptr %local.0, align 4
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = load ptr, ptr %local.1, align 8
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = add i64 0, 64
  %v.4 = call i64 @read(i32 %v.0, ptr %v.2, i64 %v.3)
  %tmp.0 = icmp slt i64 %v.4, -2147483648
  %tmp.1 = icmp sgt i64 %v.4, 2147483647
  %tmp.2 = or i1 %tmp.0, %tmp.1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.5 = trunc i64 %v.4 to i32
  ret i32 %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.9(i32 %arg.0, ptr %arg.1, i32 %arg.2) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  %local.3 = alloca i1, align 1
  %local.4 = alloca i32, align 4
  %local.5 = alloca i64, align 8
  %local.6 = alloca i1, align 1
  %local.7 = alloca i32, align 4
  %local.8 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  store ptr %arg.1, ptr %local.1, align 8
  store i32 %arg.2, ptr %local.2, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.2, align 4
  %v.1 = add i32 0, 0
  %v.2 = icmp slt i32 %v.0, %v.1
  br i1 %v.2, label %bb.1, label %bb.2
bb.1:
  %v.3 = or i1 false, true
  store i1 %v.3, ptr %local.3, align 1
  br label %bb.3
bb.2:
  %v.4 = load i32, ptr %local.2, align 4
  %v.5 = add i32 0, 64
  %v.6 = icmp sgt i32 %v.4, %v.5
  store i1 %v.6, ptr %local.3, align 1
  br label %bb.3
bb.3:
  %v.7 = load i1, ptr %local.3, align 1
  br i1 %v.7, label %bb.4, label %bb.5
bb.4:
  %v.8 = add i32 0, -2
  ret i32 %v.8
bb.5:
  br label %bb.6
bb.6:
  %v.9 = load i32, ptr %local.2, align 4
  store i32 %v.9, ptr %local.4, align 4
  br label %bb.7
bb.7:
  %v.10 = load i32, ptr %local.4, align 4
  %v.11 = add i32 0, 0
  %v.12 = icmp sgt i32 %v.10, %v.11
  br i1 %v.12, label %bb.8, label %bb.9
bb.8:
  %v.13 = add i64 0, 0
  store i64 %v.13, ptr %local.5, align 8
  %v.14 = load i32, ptr %local.0, align 4
  %v.15 = load ptr, ptr %local.1, align 8
  %v.16 = getelementptr i8, ptr %v.15, i64 0
  %v.17 = getelementptr i8, ptr %v.16, i64 0
  %v.18 = load i32, ptr %local.4, align 4
  %v.19 = sext i32 %v.18 to i64
  %v.20 = call i64 @write(i32 %v.14, ptr %v.17, i64 %v.19)
  store i64 %v.20, ptr %local.5, align 8
  %v.21 = load i64, ptr %local.5, align 8
  %v.22 = add i64 0, 0
  %v.23 = icmp sle i64 %v.21, %v.22
  br i1 %v.23, label %bb.10, label %bb.11
bb.9:
  %v.53 = add i32 0, 0
  ret i32 %v.53
bb.10:
  %v.24 = or i1 false, true
  store i1 %v.24, ptr %local.6, align 1
  br label %bb.12
bb.11:
  %v.25 = load i64, ptr %local.5, align 8
  %v.26 = load i32, ptr %local.4, align 4
  %v.27 = sext i32 %v.26 to i64
  %v.28 = icmp sgt i64 %v.25, %v.27
  store i1 %v.28, ptr %local.6, align 1
  br label %bb.12
bb.12:
  %v.29 = load i1, ptr %local.6, align 1
  br i1 %v.29, label %bb.13, label %bb.14
bb.13:
  %v.30 = add i32 0, -1
  ret i32 %v.30
bb.14:
  br label %bb.15
bb.15:
  %v.31 = load i64, ptr %local.5, align 8
  %tmp.0 = icmp slt i64 %v.31, -2147483648
  %tmp.1 = icmp sgt i64 %v.31, 2147483647
  %tmp.2 = or i1 %tmp.0, %tmp.1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.32 = trunc i64 %v.31 to i32
  store i32 %v.32, ptr %local.7, align 4
  %v.33 = load i32, ptr %local.4, align 4
  %v.34 = load i32, ptr %local.7, align 4
  %tmp.3 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.33, i32 %v.34)
  %v.35 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.35, ptr %local.4, align 4
  %v.36 = add i32 0, 0
  store i32 %v.36, ptr %local.8, align 4
  br label %bb.16
bb.16:
  %v.37 = load i32, ptr %local.8, align 4
  %v.38 = load i32, ptr %local.4, align 4
  %v.39 = icmp slt i32 %v.37, %v.38
  br i1 %v.39, label %bb.17, label %bb.18
bb.17:
  %v.40 = load ptr, ptr %local.1, align 8
  %v.41 = load i32, ptr %local.8, align 4
  %tmp.5 = sext i32 %v.41 to i64
  %tmp.6 = icmp ult i64 %tmp.5, 64
  br i1 %tmp.6, label %address.ok.2, label %zil.trap
address.ok.2:
  %v.42 = getelementptr [64 x i8], ptr %v.40, i64 0, i64 %tmp.5
  %v.43 = load ptr, ptr %local.1, align 8
  %v.44 = load i32, ptr %local.8, align 4
  %v.45 = load i32, ptr %local.7, align 4
  %tmp.7 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.44, i32 %v.45)
  %v.46 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.3
checked.3:
  %tmp.9 = sext i32 %v.46 to i64
  %tmp.10 = icmp ult i64 %tmp.9, 64
  br i1 %tmp.10, label %address.ok.4, label %zil.trap
address.ok.4:
  %v.47 = getelementptr [64 x i8], ptr %v.43, i64 0, i64 %tmp.9
  %v.48 = getelementptr i8, ptr %v.47, i64 0
  %v.49 = load i8, ptr %v.48, align 1
  store i8 %v.49, ptr %v.42, align 1
  %v.50 = load i32, ptr %local.8, align 4
  %v.51 = add i32 0, 1
  %tmp.11 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.50, i32 %v.51)
  %v.52 = extractvalue { i32, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i32, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.5
checked.5:
  store i32 %v.52, ptr %local.8, align 4
  br label %bb.16
bb.18:
  br label %bb.7
zil.trap:
  call void @llvm.trap()
  unreachable
}
define [2 x i32] @zil.fn.10() {
entry:
  %local.0 = alloca [2 x i32], align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, -1
  %v.1 = add i32 0, -1
  %tmp.0 = insertvalue [2 x i32] zeroinitializer, i32 %v.0, 0
  %v.2 = insertvalue [2 x i32] %tmp.0, i32 %v.1, 1
  store [2 x i32] %v.2, ptr %local.0, align 4
  %v.3 = getelementptr i8, ptr %local.0, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = call i32 @pipe(ptr %v.5)
  %v.7 = add i32 0, 0
  %v.8 = icmp ne i32 %v.6, %v.7
  br i1 %v.8, label %bb.1, label %bb.2
bb.1:
  %v.9 = add i32 0, -1
  %v.10 = add i32 0, -1
  %tmp.1 = insertvalue [2 x i32] zeroinitializer, i32 %v.9, 0
  %v.11 = insertvalue [2 x i32] %tmp.1, i32 %v.10, 1
  ret [2 x i32] %v.11
bb.2:
  br label %bb.3
bb.3:
  %v.12 = load [2 x i32], ptr %local.0, align 4
  ret [2 x i32] %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}
define void @zil.fn.11(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call i32 @close(i32 %v.0)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i1 @zil.fn.12(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, -1
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = call i32 @dup(i32 %v.1)
  store i32 %v.2, ptr %local.1, align 4
  %v.3 = load i32, ptr %local.1, align 4
  %v.4 = add i32 0, 0
  %v.5 = icmp sge i32 %v.3, %v.4
  br i1 %v.5, label %bb.1, label %bb.2
bb.1:
  %v.6 = load i32, ptr %local.1, align 4
  call void @zil.fn.11(i32 %v.6)
  %v.8 = or i1 false, false
  ret i1 %v.8
bb.2:
  br label %bb.3
bb.3:
  %v.9 = or i1 false, true
  ret i1 %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}
define void @zil.fn.13(i32 %arg.0) {
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
define void @zil.fn.14(i64 %arg.0) {
entry:
  %local.0 = alloca i64, align 8
  store i64 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load i64, ptr %local.0, align 8
  %v.1 = add i64 0, 10
  %v.2 = icmp sge i64 %v.0, %v.1
  br i1 %v.2, label %bb.1, label %bb.2
bb.1:
  %v.3 = load i64, ptr %local.0, align 8
  %v.4 = add i64 0, 10
  %tmp.0 = icmp eq i64 %v.4, 0
  %tmp.1 = icmp eq i64 %v.3, -9223372036854775808
  %tmp.2 = icmp eq i64 %v.4, -1
  %tmp.3 = and i1 %tmp.1, %tmp.2
  %tmp.4 = or i1 %tmp.0, %tmp.3
  br i1 %tmp.4, label %zil.trap, label %checked.0
checked.0:
  %v.5 = sdiv i64 %v.3, %v.4
  call void @zil.fn.14(i64 %v.5)
  br label %bb.3
bb.2:
  br label %bb.3
bb.3:
  %v.7 = add i32 0, 48
  %v.8 = load i64, ptr %local.0, align 8
  %v.9 = add i64 0, 10
  %tmp.5 = icmp eq i64 %v.9, 0
  %tmp.6 = icmp eq i64 %v.8, -9223372036854775808
  %tmp.7 = icmp eq i64 %v.9, -1
  %tmp.8 = and i1 %tmp.6, %tmp.7
  %tmp.9 = or i1 %tmp.5, %tmp.8
  br i1 %tmp.9, label %zil.trap, label %checked.1
checked.1:
  %v.10 = srem i64 %v.8, %v.9
  %tmp.10 = icmp slt i64 %v.10, -2147483648
  %tmp.11 = icmp sgt i64 %v.10, 2147483647
  %tmp.12 = or i1 %tmp.10, %tmp.11
  br i1 %tmp.12, label %zil.trap, label %checked.2
checked.2:
  %v.11 = trunc i64 %v.10 to i32
  %tmp.13 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.7, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.13, 0
  %tmp.14 = extractvalue { i32, i1 } %tmp.13, 1
  br i1 %tmp.14, label %zil.trap, label %checked.3
checked.3:
  call void @zil.fn.13(i32 %v.12)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}
define void @zil.fn.15(i64 %arg.0) {
entry:
  %local.0 = alloca i64, align 8
  store i64 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load i64, ptr %local.0, align 8
  call void @zil.fn.14(i64 %v.0)
  %v.2 = add i32 0, 10
  call void @zil.fn.13(i32 %v.2)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
