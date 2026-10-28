; ZIL verified LLVM module
; verified-fingerprint: 24e5e9154811ad21bdcbc7a2fb4e80d8a968bc84a827f912b71b4cdc9d85eba9
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
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
  %local.1 = alloca i32, align 4
  %local.2 = alloca %zil_env_0, align 4
  br label %bb.0
bb.0:
  %v.0 = call i32 @zil.fn.1()
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.1, align 4
  %v.3 = insertvalue %zil_env_0 zeroinitializer, i32 %v.2, 0
  store %zil_env_0 %v.3, ptr %local.2, align 4
  %v.4 = getelementptr i8, ptr %local.2, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = load i32, ptr %local.0, align 4
  %v.8 = call i32 @zil.fn.6(ptr %v.6, i32 %v.7)
  %v.9 = load i32, ptr %local.0, align 4
  %v.10 = load i32, ptr %local.0, align 4
  %v.11 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.10, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %tmp.2 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.9, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %v.14 = add i32 0, 2
  %tmp.4 = icmp eq i32 %v.14, 0
  %tmp.5 = icmp eq i32 %v.13, -2147483648
  %tmp.6 = icmp eq i32 %v.14, -1
  %tmp.7 = and i1 %tmp.5, %tmp.6
  %tmp.8 = or i1 %tmp.4, %tmp.7
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  %v.15 = sdiv i32 %v.13, %v.14
  %tmp.9 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.8, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i32, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.3
checked.3:
  ret i32 %v.16
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = add i32 0, 1
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.2)
  %v.3 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.3, ptr %local.0, align 4
  %tmp.3 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store i32 %v.3, ptr %tmp.3, align 4
  %v.4 = add i32 0, 64
  %v.5 = load i32, ptr %local.0, align 4
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  call void @zil.fn.3(i32 %v.6)
  %v.8 = load i32, ptr %local.0, align 4
  ret i32 %v.8
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
  %local.5 = alloca i32, align 4
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
  %v.8 = add i32 0, 0
  %v.9 = add i32 0, 4
  store i32 %v.8, ptr %local.4, align 4
  br label %bb.4
bb.3:
  %v.24 = load i32, ptr %local.3, align 4
  ret i32 %v.24
bb.4:
  %v.10 = load i32, ptr %local.4, align 4
  %v.11 = icmp slt i32 %v.10, %v.9
  br i1 %v.11, label %bb.5, label %bb.7
bb.5:
  store i32 %v.10, ptr %local.5, align 4
  %v.12 = load i32, ptr %local.5, align 4
  %v.13 = add i32 0, 0
  %v.14 = icmp eq i32 %v.12, %v.13
  br i1 %v.14, label %bb.8, label %bb.9
bb.6:
  %v.22 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.10, i32 %v.22)
  %v.23 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.23, ptr %local.4, align 4
  br label %bb.4
bb.7:
  br label %bb.1
bb.8:
  br label %bb.6
bb.9:
  br label %bb.10
bb.10:
  %v.15 = load i32, ptr %local.5, align 4
  %v.16 = add i32 0, 2
  %v.17 = icmp eq i32 %v.15, %v.16
  br i1 %v.17, label %bb.11, label %bb.12
bb.11:
  br label %bb.7
bb.12:
  br label %bb.13
bb.13:
  %v.18 = load i32, ptr %local.3, align 4
  %v.19 = load ptr, ptr %local.0, align 8
  %v.20 = call i32 @zil.fn.5(ptr %v.19)
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.18, i32 %v.20)
  %v.21 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.21, ptr %local.3, align 4
  br label %bb.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.4()
  ret i32 %entry.result
}
