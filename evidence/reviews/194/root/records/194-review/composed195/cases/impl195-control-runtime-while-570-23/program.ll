; ZIL verified LLVM module
; verified-fingerprint: 9fa2df85f2b7178bcea634a50bec5b972683536cff2bcbff863eafde2a76162b
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
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
  %v.2 = getelementptr i8, ptr %local.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = load i32, ptr %local.0, align 4
  %v.6 = call i32 @zil.fn.6(ptr %v.4, i32 %v.5)
  store i32 %v.6, ptr %local.2, align 4
  %v.7 = add i32 0, 65
  %v.8 = load i32, ptr %local.2, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.7, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  call void @zil.fn.3(i32 %v.9)
  %v.11 = add i32 0, 0
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(%zil_env_0 %env, i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 48
  %v.1 = load i32, ptr %local.0, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.0, i32 %v.1)
  %v.2 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  call void @zil.fn.3(i32 %v.2)
  %v.4 = load i32, ptr %local.0, align 4
  ret i32 %v.4
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
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.3, align 4
  br label %bb.1
bb.1:
  %v.2 = load i32, ptr %local.2, align 4
  %v.3 = load i32, ptr %local.1, align 4
  %v.4 = icmp slt i32 %v.2, %v.3
  br i1 %v.4, label %bb.2, label %bb.3
bb.2:
  %v.5 = load i32, ptr %local.2, align 4
  %v.6 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.7, ptr %local.2, align 4
  %v.8 = load i32, ptr %local.2, align 4
  %v.9 = add i32 0, 2
  %v.10 = icmp eq i32 %v.8, %v.9
  br i1 %v.10, label %bb.4, label %bb.5
bb.3:
  %v.19 = load i32, ptr %local.3, align 4
  ret i32 %v.19
bb.4:
  br label %bb.1
bb.5:
  br label %bb.6
bb.6:
  %v.11 = load i32, ptr %local.2, align 4
  %v.12 = add i32 0, 5
  %v.13 = icmp eq i32 %v.11, %v.12
  br i1 %v.13, label %bb.7, label %bb.8
bb.7:
  br label %bb.3
bb.8:
  br label %bb.9
bb.9:
  %v.14 = load i32, ptr %local.3, align 4
  %v.15 = load ptr, ptr %local.0, align 8
  %v.16 = load i32, ptr %local.2, align 4
  %tmp.2 = load %zil_env_0, ptr %v.15, align 1
  %v.17 = call i32 @zil.fn.5(%zil_env_0 %tmp.2, i32 %v.16)
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.14, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.18, ptr %local.3, align 4
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
