; ZIL verified LLVM module
; verified-fingerprint: 7ce2ba46e2d6016af81428c9ddaf7dcf042400e91736c7b7aa3d6b0650ba8d37
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
%zil_env_1 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca %zil_env_0, align 1
  %local.2 = alloca %zil_env_1, align 1
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 41
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.1, ptr %local.1, align 1
  %v.2 = freeze %zil_env_1 zeroinitializer
  store %zil_env_1 %v.2, ptr %local.2, align 1
  %v.3 = load %zil_env_0, ptr %local.1, align 1
  %v.4 = getelementptr i8, ptr %local.0, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = call i32 @zil.fn.1(%zil_env_0 %v.3, ptr %v.6)
  store i32 %v.7, ptr %local.3, align 4
  %v.8 = load %zil_env_1, ptr %local.2, align 1
  %v.9 = getelementptr i8, ptr %local.0, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = call i32 @zil.fn.2(%zil_env_1 %v.8, ptr %v.11)
  store i32 %v.12, ptr %local.4, align 4
  %v.13 = load i32, ptr %local.3, align 4
  %v.14 = load i32, ptr %local.4, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.16 = load i32, ptr %local.0, align 4
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.15, i32 %v.16)
  %v.17 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %v.18 = add i32 0, 125
  %tmp.4 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.17, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %v.0, align 4
  ret i32 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(%zil_env_1 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = load i32, ptr %v.1, align 4
  %v.3 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.4, ptr %v.0, align 4
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = load i32, ptr %v.5, align 4
  ret i32 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
