; ZIL verified LLVM module
; verified-fingerprint: 52da1c85dd2b0270017e8d8672ec9ddbddd22c1d0abb3703495cb1ba0fb0116a
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { ptr }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
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
  %local.2 = alloca { i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { i32 }, { i32 } }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { { i32 }, { i32 } }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { { { i32 }, { i32 } } }, align 4
  %local.7 = alloca %zil_env_0, align 8
  %local.8 = alloca i32, align 4
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { { { i32 }, { i32 } } }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = add i32 0, 65
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i32 0, 7
  %v.4 = call { i32 } @zil.fn.5(i32 %v.3)
  store { i32 } %v.4, ptr %local.2, align 4
  %v.5 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.5, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.6 = load { i32 }, ptr %local.2, align 4
  %v.7 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %tmp.0 = insertvalue { { i32 }, { i32 } } zeroinitializer, { i32 } %v.7, 0
  %v.8 = insertvalue { { i32 }, { i32 } } %tmp.0, { i32 } %v.6, 1
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
  store { { i32 }, { i32 } } %v.8, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.9 = load { { i32 }, { i32 } }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  store { { i32 }, { i32 } } %v.9, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.10 = load { { i32 }, { i32 } }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  %v.11 = insertvalue { { { i32 }, { i32 } } } zeroinitializer, { { i32 }, { i32 } } %v.10, 0
  %tmp.3 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.3, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.4 = getelementptr { { i32 }, { i32 } }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.4)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.5 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.5, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.6 = getelementptr { { i32 }, { i32 } }, ptr %local.4, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.6)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.6, align 1
  store { { { i32 }, { i32 } } } %v.11, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.12 = getelementptr i8, ptr %local.6, i64 0
  %v.13 = getelementptr i8, ptr %v.12, i64 0
  %v.14 = insertvalue %zil_env_0 zeroinitializer, ptr %v.13, 0
  store %zil_env_0 %v.14, ptr %local.7, align 8
  %v.15 = load %zil_env_0, ptr %local.7, align 8
  %v.16 = call i32 @zil.fn.6(%zil_env_0 %v.15)
  %v.17 = load %zil_env_0, ptr %local.7, align 8
  %v.18 = call i32 @zil.fn.6(%zil_env_0 %v.17)
  %tmp.7 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.16, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.4
checked.4:
  store i32 %v.19, ptr %local.8, align 4
  %v.20 = load { { { i32 }, { i32 } } }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.9, align 1
  store { { { i32 }, { i32 } } } %v.20, ptr %local.9, align 4
  store i1 true, ptr %resource.live.9, align 1
  %v.21 = load i32, ptr %local.8, align 4
  %v.22 = add i32 0, 14
  %v.23 = icmp ne i32 %v.21, %v.22
  br i1 %v.23, label %bb.1, label %bb.2
bb.1:
  %v.24 = add i32 0, 1
  %tmp.9 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.9, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.10 = getelementptr { { { i32 }, { i32 } } }, ptr %local.9, i32 0, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.10)
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.11 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.11, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.12 = getelementptr { { { i32 }, { i32 } } }, ptr %local.6, i32 0, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.12)
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.6, align 1
  ret i32 %v.24
bb.2:
  br label %bb.3
bb.3:
  %v.25 = add i32 0, 0
  %tmp.13 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.13, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.14 = getelementptr { { { i32 }, { i32 } } }, ptr %local.9, i32 0, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.14)
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.15 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.15, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.16 = getelementptr { { { i32 }, { i32 } } }, ptr %local.6, i32 0, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.16)
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.6, align 1
  ret i32 %v.25
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.3(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = call i32 @zil.fn.4(ptr %v.1)
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.4(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}
define { i32 } @zil.fn.5(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  ret { i32 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.6(%zil_env_0 %env) {
entry:
  %local.0 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = extractvalue %zil_env_0 %env, 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = getelementptr { { { i32 }, { i32 } } }, ptr %v.1, i32 0, i32 0
  %v.3 = getelementptr { { i32 }, { i32 } }, ptr %v.2, i32 0, i32 1
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = call i32 @zil.fn.3(ptr %v.5)
  ret i32 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.2()
  ret i32 %entry.result
}
