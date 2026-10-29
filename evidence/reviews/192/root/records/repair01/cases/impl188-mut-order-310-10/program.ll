; ZIL verified LLVM module
; verified-fingerprint: 26b5f287d85ee297263a3d9f66b5e333c5fb1333b75a79fd36090a4048f6ef56
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca %zil_env_0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = insertvalue %zil_env_0 zeroinitializer, i32 %v.1, 0
  store %zil_env_0 %v.2, ptr %local.1, align 4
  %v.3 = getelementptr i8, ptr %local.1, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = add i32 0, 1
  %v.7 = call i32 @zil.fn.4(ptr %v.5, i32 %v.6)
  %v.8 = add i32 0, 10
  %v.9 = icmp ne i32 %v.7, %v.8
  br i1 %v.9, label %bb.1, label %bb.2
bb.1:
  %v.10 = add i32 0, 1
  ret i32 %v.10
bb.2:
  br label %bb.3
bb.3:
  %v.11 = load %zil_env_0, ptr %local.1, align 4
  %v.12 = add i32 0, 0
  %v.13 = call i32 @zil.fn.1(ptr %local.1, i32 %v.12)
  %v.14 = add i32 0, 6
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.15
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(ptr %env, i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.2)
  %v.3 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.3, ptr %local.1, align 4
  %tmp.3 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store i32 %v.3, ptr %tmp.3, align 4
  %v.4 = load i32, ptr %local.1, align 4
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = call i32 @zil.fn.1(ptr %v.0, i32 %v.1)
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load i32, ptr %local.1, align 4
  %v.3 = call i32 @zil.fn.2(ptr %v.1, i32 %v.2)
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = load ptr, ptr %local.0, align 8
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = load i32, ptr %local.1, align 4
  %v.7 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.9 = call i32 @zil.fn.2(ptr %v.5, i32 %v.8)
  %v.10 = load i32, ptr %local.2, align 4
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load i32, ptr %local.1, align 4
  %v.3 = call i32 @zil.fn.3(ptr %v.1, i32 %v.2)
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = load ptr, ptr %local.0, align 8
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = load i32, ptr %local.1, align 4
  %v.7 = add i32 0, 2
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.9 = call i32 @zil.fn.2(ptr %v.5, i32 %v.8)
  %v.10 = load i32, ptr %local.2, align 4
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
