; ZIL verified LLVM module
; verified-fingerprint: 58b6b866411b52c963bd1f5c6393145d4c5183d08f8f1178069f6c621203c3ed
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.1(i32 %arg.0) {
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

define void @zil.fn.2(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.1(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3() {
entry:
  %local.0 = alloca %zil_env_0, align 1
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i32 0, 3
  %v.5 = call i32 @zil.fn.5(ptr %v.3, i32 %v.4)
  %v.6 = add i32 0, 2
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

define i32 @zil.fn.4(%zil_env_0 %env) {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 67
  call void @zil.fn.1(i32 %v.0)
  %v.2 = add i32 0, 1
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
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
  %v.5 = add i32 0, 65
  %v.6 = load i32, ptr %local.2, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.7, ptr %local.4, align 4
  %v.8 = load i32, ptr %local.4, align 4
%v.9 = insertvalue { i32 } zeroinitializer, i32 %v.8, 0
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %v.9, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.10 = load i32, ptr %local.3, align 4
  %v.11 = load ptr, ptr %local.0, align 8
  %tmp.2 = load %zil_env_0, ptr %v.11, align 1
  %v.12 = call i32 @zil.fn.4(%zil_env_0 %tmp.2)
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.10, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.13, ptr %local.3, align 4
  %v.14 = load i32, ptr %local.2, align 4
  %v.15 = add i32 0, 1
  %v.16 = icmp eq i32 %v.14, %v.15
  br i1 %v.16, label %bb.4, label %bb.5
bb.3:
  %v.20 = load i32, ptr %local.3, align 4
  ret i32 %v.20
bb.4:
  %tmp.5 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.5, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.2(ptr %local.5)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.5, align 1
  br label %bb.3
bb.5:
  br label %bb.6
bb.6:
  %v.17 = load i32, ptr %local.2, align 4
  %v.18 = add i32 0, 1
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.17, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  store i32 %v.19, ptr %local.2, align 4
  %tmp.8 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.8, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.2(ptr %local.5)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.5, align 1
  br label %bb.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.3()
  ret i32 %entry.result
}
