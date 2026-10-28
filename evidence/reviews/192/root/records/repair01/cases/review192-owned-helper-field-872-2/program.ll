; ZIL verified LLVM module
; verified-fingerprint: f2ea6758ec012a374ecc41129b75e482423ab8c24f4d16665349bee5a622f123
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { { i32 }, i64 }, i1, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %field.flag.0.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %field.live.0.0 = load i1, ptr %field.flag.0.0, align 1
  br i1 %field.live.0.0, label %field.drop.0.0, label %field.done.0.0
field.drop.0.0:
  store i1 false, ptr %field.flag.0.0, align 1
  %payload.0.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0, i32 0
  call void @zil.fn.1(ptr %payload.0.0)
  br label %field.done.0.0
field.done.0.0:
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  %v.2 = call i32 @putchar(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i64, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { i32 }, i64 }, align 8
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %local.7 = alloca i64, align 8
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { i32 }, align 4
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { { i32 }, i64 }, align 8
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca %zil_env_0, align 8
  %local.11 = alloca ptr, align 8
  %local.12 = alloca i64, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = add i32 0, 66
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i64 0, 20
  store i64 %v.3, ptr %local.2, align 8
  %v.4 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.4, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.5 = load i64, ptr %local.2, align 8
  %v.6 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
%tmp.0 = insertvalue { { i32 }, i64 } zeroinitializer, { i32 } %v.6, 0
%v.7 = insertvalue { { i32 }, i64 } %tmp.0, i64 %v.5, 1
  %tmp.1 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  store { { i32 }, i64 } %v.7, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.8, align 1
  %v.8 = add i32 0, 65
  store i32 %v.8, ptr %local.5, align 4
  %v.9 = load i32, ptr %local.5, align 4
%v.10 = insertvalue { i32 } zeroinitializer, i32 %v.9, 0
  store i1 false, ptr %resource.live.6, align 1
  store { i32 } %v.10, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.11 = add i64 0, 10
  store i64 %v.11, ptr %local.7, align 8
  %v.12 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  store { i32 } %v.12, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %v.13 = load i64, ptr %local.7, align 8
  %v.14 = load { i32 }, ptr %local.8, align 4
  store i1 false, ptr %resource.live.8, align 1
%tmp.3 = insertvalue { { i32 }, i64 } zeroinitializer, { i32 } %v.14, 0
%v.15 = insertvalue { { i32 }, i64 } %tmp.3, i64 %v.13, 1
  %tmp.4 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.1(ptr %local.8)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.5 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.1(ptr %local.6)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.9, align 1
  store { { i32 }, i64 } %v.15, ptr %local.9, align 8
  store i1 true, ptr %resource.live.9, align 1
  %v.16 = load { { i32 }, i64 }, ptr %local.9, align 8
  store i1 false, ptr %resource.live.9, align 1
  %tmp.6 = insertvalue %zil_env_0 zeroinitializer, { { i32 }, i64 } %v.16, 0
  %tmp.7 = insertvalue %zil_env_0 %tmp.6, i1 true, 1
  %v.17 = insertvalue %zil_env_0 %tmp.7, i1 true, 2
  store i1 false, ptr %resource.live.10, align 1
  store %zil_env_0 %v.17, ptr %local.10, align 8
  store i1 true, ptr %resource.live.10, align 1
  %v.18 = getelementptr { { i32 }, i64 }, ptr %local.4, i32 0, i32 1
  %v.19 = getelementptr i8, ptr %v.18, i64 0
  store ptr %v.19, ptr %local.11, align 8
  %v.20 = getelementptr i8, ptr %local.10, i64 0
  %v.21 = getelementptr i8, ptr %v.20, i64 0
  %v.22 = getelementptr i8, ptr %v.21, i64 0
  %v.23 = load ptr, ptr %local.11, align 8
  %v.24 = getelementptr i8, ptr %v.23, i64 0
  %v.25 = call i64 @zil.fn.5(ptr %v.22, ptr %v.24)
  store i64 %v.25, ptr %local.12, align 8
  %v.26 = load i64, ptr %local.12, align 8
  %v.27 = load ptr, ptr %local.11, align 8
  %v.28 = load i64, ptr %v.27, align 8
  %tmp.8 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.26, i64 %v.28)
  %v.29 = extractvalue { i64, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i64, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.4
checked.4:
  %tmp.10 = icmp slt i64 %v.29, -2147483648
  %tmp.11 = icmp sgt i64 %v.29, 2147483647
  %tmp.12 = or i1 %tmp.10, %tmp.11
  br i1 %tmp.12, label %zil.trap, label %checked.5
checked.5:
  %v.30 = trunc i64 %v.29 to i32
  %v.31 = add i32 0, 103
  %tmp.13 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.30, i32 %v.31)
  %v.32 = extractvalue { i32, i1 } %tmp.13, 0
  %tmp.14 = extractvalue { i32, i1 } %tmp.13, 1
  br i1 %tmp.14, label %zil.trap, label %checked.6
checked.6:
  %tmp.15 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.15, label %closure.drop.7, label %closure.done.7
closure.drop.7:
  store i1 false, ptr %resource.live.10, align 1
  call void @zil.closure.drop.0(ptr %local.10)
  br label %closure.done.7
closure.done.7:
  store i1 false, ptr %resource.live.10, align 1
  %tmp.16 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.16, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.17 = getelementptr { { i32 }, i64 }, ptr %local.9, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.17)
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.18 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.18, label %package.drop.9, label %package.done.9
package.drop.9:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.19 = getelementptr { { i32 }, i64 }, ptr %local.4, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.19)
  br label %package.done.9
package.done.9:
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.32
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.3(ptr %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %resource.live.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %package.live.1.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %local.2 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr { { i32 }, i64 }, ptr %local.1, i32 0, i32 1
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  store ptr %v.1, ptr %local.2, align 8
  %v.2 = load ptr, ptr %local.2, align 8
  %v.3 = load ptr, ptr %local.2, align 8
  %v.4 = load i64, ptr %v.3, align 8
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = load i64, ptr %v.5, align 8
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.4, i64 %v.6)
  %v.7 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i64 %v.7, ptr %v.2, align 8
  %v.8 = load ptr, ptr %local.0, align 8
  %v.9 = load ptr, ptr %local.0, align 8
  %v.10 = load i64, ptr %v.9, align 8
  %v.11 = add i64 0, 1
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.10, i64 %v.11)
  %v.12 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i64 %v.12, ptr %v.8, align 8
  %v.13 = load ptr, ptr %local.2, align 8
  %v.14 = load i64, ptr %v.13, align 8
  ret i64 %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.4(ptr %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = call i64 @zil.fn.3(ptr %v.0, ptr %v.2)
  ret i64 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.5(ptr %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load ptr, ptr %local.1, align 8
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = call i64 @zil.fn.4(ptr %v.1, ptr %v.3)
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = load ptr, ptr %local.1, align 8
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = call i64 @zil.fn.4(ptr %v.6, ptr %v.8)
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.4, i64 %v.9)
  %v.10 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i64 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.2()
  ret i32 %entry.result
}
