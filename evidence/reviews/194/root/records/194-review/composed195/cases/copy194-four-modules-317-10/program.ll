; ZIL verified LLVM module
; verified-fingerprint: 1854d4a7113cb906ca391bc7e7cf27c71356d7b44e31f8894d4038e806243ff9
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca %zil_env_0, align 1
  %local.1 = alloca i32, align 4
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  %v.0 = call %zil_env_0 @zil.fn.1()
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i32 0, 3
  store i32 %v.4, ptr %local.1, align 4
  %v.5 = load i32, ptr %local.1, align 4
%v.6 = insertvalue { i32 } zeroinitializer, i32 %v.5, 0
  %v.7 = call { i32 } @zil.fn.3(ptr %v.3, { i32 } %v.6)
  store { i32 } %v.7, ptr %local.2, align 4
  %v.8 = getelementptr i8, ptr %local.0, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = add i32 0, 4
  store i32 %v.11, ptr %local.3, align 4
  %v.12 = load i32, ptr %local.3, align 4
%v.13 = insertvalue { i32 } zeroinitializer, i32 %v.12, 0
  %v.14 = call { i32 } @zil.fn.4(ptr %v.10, { i32 } %v.13)
  store { i32 } %v.14, ptr %local.4, align 4
  %v.15 = load { i32 }, ptr %local.2, align 4
%v.16 = extractvalue { i32 } %v.15, 0
  %v.17 = load { i32 }, ptr %local.4, align 4
%v.18 = extractvalue { i32 } %v.17, 0
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.16, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.20 = add i32 0, 7
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.19, i32 %v.20)
  %v.21 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.21
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.1() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  ret %zil_env_0 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(%zil_env_0 %env) {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 1
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i32 } @zil.fn.3(ptr %arg.0, { i32 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { i32 } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.2(%zil_env_0 %tmp.0)
  %v.2 = load { i32 }, ptr %local.1, align 4
  ret { i32 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i32 } @zil.fn.4(ptr %arg.0, { i32 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { i32 } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.2(%zil_env_0 %tmp.0)
  %v.2 = load { i32 }, ptr %local.1, align 4
  ret { i32 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
