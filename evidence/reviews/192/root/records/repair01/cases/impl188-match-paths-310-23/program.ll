; ZIL verified LLVM module
; verified-fingerprint: f51231775a97e59ad2ced3a2d63541a2053f9c3ece4e9a08a6746acae2b62b3d
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
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = or i1 false, true
  %v.5 = call i32 @zil.fn.4(ptr %v.3, i1 %v.4)
  %v.6 = add i32 0, 8
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env) {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 4
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(ptr %arg.0, i1 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i1, align 1
  store ptr %arg.0, ptr %local.0, align 8
  store i1 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  %v.0 = load i1, ptr %local.1, align 1
  %v.1 = or i1 false, true
  %v.2 = icmp eq i1 %v.0, %v.1
  br i1 %v.2, label %bb.2, label %bb.1
bb.1:
  %v.5 = or i1 false, false
  %v.6 = icmp eq i1 %v.0, %v.5
  br i1 %v.6, label %bb.4, label %bb.3
bb.2:
  %v.3 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.3, align 1
  %v.4 = call i32 @zil.fn.1(%zil_env_0 %tmp.0)
  ret i32 %v.4
bb.3:
  br label %zil.trap
bb.4:
  %v.7 = add i32 0, 0
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3(ptr %arg.0, i1 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i1, align 1
  %local.2 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i1 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load i1, ptr %local.1, align 1
  %v.3 = call i32 @zil.fn.2(ptr %v.1, i1 %v.2)
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.4, align 1
  %v.5 = call i32 @zil.fn.1(%zil_env_0 %tmp.0)
  %v.6 = load i32, ptr %local.2, align 4
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4(ptr %arg.0, i1 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i1, align 1
  store ptr %arg.0, ptr %local.0, align 8
  store i1 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load i1, ptr %local.1, align 1
  %v.3 = call i32 @zil.fn.3(ptr %v.1, i1 %v.2)
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
