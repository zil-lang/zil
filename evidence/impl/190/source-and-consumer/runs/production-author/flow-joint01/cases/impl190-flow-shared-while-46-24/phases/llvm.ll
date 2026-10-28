; ZIL verified LLVM module
; verified-fingerprint: 3efc439d3af76ee1bd02ae3f6cb40680ddeed07df855e2964866334fca78f2cf
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
  %v.7 = add i32 0, 3
  %v.8 = load i32, ptr %local.0, align 4
  %tmp.0 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.7, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.6, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.10
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
  %v.5 = load i32, ptr %local.3, align 4
  %v.6 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.6, align 1
  %v.7 = call i32 @zil.fn.5(%zil_env_0 %tmp.0)
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.8, ptr %local.3, align 4
  %v.9 = load i32, ptr %local.2, align 4
  %v.10 = add i32 0, 1
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.11, ptr %local.2, align 4
  br label %bb.1
bb.3:
  %v.12 = load i32, ptr %local.3, align 4
  ret i32 %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.4()
  ret i32 %entry.result
}
