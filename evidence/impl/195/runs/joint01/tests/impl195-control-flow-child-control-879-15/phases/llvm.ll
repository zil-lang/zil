; ZIL verified LLVM module
; verified-fingerprint: aab0b595ae7a698a07bf94bf4b7319b2367da36a21b3aff0080d87e34e2b2972
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar() nonlazybind
define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
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
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.3(i32 %arg.0) {
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

define i32 @zil.fn.4() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca %zil_env_0, align 1
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = call i32 @zil.fn.1()
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.1, ptr %local.1, align 1
  %v.2 = load i32, ptr %local.0, align 4
  store i32 %v.2, ptr %local.2, align 4
  %v.3 = load i32, ptr %local.0, align 4
  %v.4 = add i32 0, 2
  %v.5 = icmp sge i32 %v.3, %v.4
  br i1 %v.5, label %bb.1, label %bb.2
bb.1:
  %v.6 = load i32, ptr %local.0, align 4
  %v.7 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.6, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.8, ptr %local.2, align 4
  br label %bb.3
bb.2:
  br label %bb.3
bb.3:
  %v.9 = getelementptr i8, ptr %local.1, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = load i32, ptr %local.0, align 4
  %v.13 = call i32 @zil.fn.7(ptr %v.11, i32 %v.12)
  %v.14 = add i32 0, 6
  %v.15 = load i32, ptr %local.2, align 4
  %tmp.2 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.14, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %tmp.4 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.13, i32 %v.16)
  %v.17 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(%zil_env_0 %env) {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 65
  call void @zil.fn.3(i32 %v.0)
  %v.2 = add i32 0, 3
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6(ptr %arg.0, i32 %arg.1) {
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
  %v.5 = load i32, ptr %local.4, align 4
  %v.6 = add i32 0, 0
  %v.7 = icmp eq i32 %v.5, %v.6
  br i1 %v.7, label %bb.5, label %bb.6
bb.3:
  %v.15 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.16, ptr %local.3, align 4
  br label %bb.1
bb.4:
  %v.17 = load i32, ptr %local.2, align 4
  ret i32 %v.17
bb.5:
  br label %bb.3
bb.6:
  br label %bb.7
bb.7:
  %v.8 = load i32, ptr %local.4, align 4
  %v.9 = add i32 0, 3
  %v.10 = icmp eq i32 %v.8, %v.9
  br i1 %v.10, label %bb.8, label %bb.9
bb.8:
  br label %bb.4
bb.9:
  br label %bb.10
bb.10:
  %v.11 = load i32, ptr %local.2, align 4
  %v.12 = load ptr, ptr %local.0, align 8
  %tmp.2 = load %zil_env_0, ptr %v.12, align 1
  %v.13 = call i32 @zil.fn.5(%zil_env_0 %tmp.2)
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.14, ptr %local.2, align 4
  br label %bb.3
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
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.1, align 4
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.3, align 4
  br label %bb.1
bb.1:
  %v.2 = load i32, ptr %local.2, align 4
  %v.3 = add i32 0, 0
  %v.4 = icmp sgt i32 %v.2, %v.3
  br i1 %v.4, label %bb.2, label %bb.3
bb.2:
  %v.5 = load i32, ptr %local.2, align 4
  %v.6 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.7, ptr %local.2, align 4
  %v.8 = load i32, ptr %local.2, align 4
  %v.9 = add i32 0, 1
  %v.10 = icmp eq i32 %v.8, %v.9
  br i1 %v.10, label %bb.4, label %bb.5
bb.3:
  %v.20 = load i32, ptr %local.3, align 4
  ret i32 %v.20
bb.4:
  br label %bb.1
bb.5:
  br label %bb.6
bb.6:
  %v.11 = load i32, ptr %local.3, align 4
  %v.12 = load ptr, ptr %local.0, align 8
  %v.13 = getelementptr i8, ptr %v.12, i64 0
  %v.14 = add i32 0, 5
  %v.15 = call i32 @zil.fn.6(ptr %v.13, i32 %v.14)
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.16, ptr %local.3, align 4
  %v.17 = load i32, ptr %local.2, align 4
  %v.18 = add i32 0, 0
  %v.19 = icmp eq i32 %v.17, %v.18
  br i1 %v.19, label %bb.7, label %bb.8
bb.7:
  br label %bb.3
bb.8:
  br label %bb.9
bb.9:
  br label %bb.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.4()
  ret i32 %entry.result
}
