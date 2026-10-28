; ZIL verified LLVM module
; verified-fingerprint: 82ff22ce415b1eedce53c924adbd2b6f2b2c208eb9a961250c8dfa2a1514444e
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i64, i1 } @llvm.ssub.with.overflow.i64(i64, i64)

define ptr @zil.fn.0(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i64, i8 }, ptr %v.0, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  ret ptr %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca {  }, align 1
  %local.3 = alloca i8, align 1
  %local.4 = alloca i64, align 8
  %local.5 = alloca i8, align 1
  %local.6 = alloca { i64, i8 }, align 8
  %local.7 = alloca i64, align 8
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { i32 }, align 4
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { { i32 }, {  }, i8, { i64, i8 }, i64 }, align 8
  %local.10 = alloca ptr, align 8
  %local.11 = alloca ptr, align 8
  %local.12 = alloca ptr, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.8, align 1
  %v.0 = add i32 0, 1
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
%v.3 = freeze {  } zeroinitializer
  store {  } %v.3, ptr %local.2, align 1
  %v.4 = add i8 0, 7
  store i8 %v.4, ptr %local.3, align 1
  %v.5 = add i64 0, 5000000000
  store i64 %v.5, ptr %local.4, align 8
  %v.6 = add i8 0, 9
  store i8 %v.6, ptr %local.5, align 1
  %v.7 = load i64, ptr %local.4, align 8
  %v.8 = load i8, ptr %local.5, align 1
%tmp.0 = insertvalue { i64, i8 } zeroinitializer, i64 %v.7, 0
%v.9 = insertvalue { i64, i8 } %tmp.0, i8 %v.8, 1
  store { i64, i8 } %v.9, ptr %local.6, align 8
  %v.10 = add i64 0, 11
  store i64 %v.10, ptr %local.7, align 8
  %v.11 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.11, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %v.12 = load {  }, ptr %local.2, align 1
  %v.13 = load i8, ptr %local.3, align 1
  %v.14 = load { i64, i8 }, ptr %local.6, align 8
  %v.15 = load i64, ptr %local.7, align 8
  %v.16 = load { i32 }, ptr %local.8, align 4
  store i1 false, ptr %resource.live.8, align 1
%tmp.1 = insertvalue { { i32 }, {  }, i8, { i64, i8 }, i64 } zeroinitializer, { i32 } %v.16, 0
%tmp.2 = insertvalue { { i32 }, {  }, i8, { i64, i8 }, i64 } %tmp.1, {  } %v.12, 1
%tmp.3 = insertvalue { { i32 }, {  }, i8, { i64, i8 }, i64 } %tmp.2, i8 %v.13, 2
%tmp.4 = insertvalue { { i32 }, {  }, i8, { i64, i8 }, i64 } %tmp.3, { i64, i8 } %v.14, 3
%v.17 = insertvalue { { i32 }, {  }, i8, { i64, i8 }, i64 } %tmp.4, i64 %v.15, 4
  %tmp.5 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.5, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.8, align 1
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.6 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.6, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.9, align 1
  store { { i32 }, {  }, i8, { i64, i8 }, i64 } %v.17, ptr %local.9, align 8
  store i1 true, ptr %resource.live.9, align 1
  %v.18 = getelementptr { { i32 }, {  }, i8, { i64, i8 }, i64 }, ptr %local.9, i32 0, i32 1
  %v.19 = getelementptr i8, ptr %v.18, i64 0
  store ptr %v.19, ptr %local.10, align 8
  %v.20 = load ptr, ptr %local.10, align 8
%v.21 = freeze {  } zeroinitializer
  store {  } %v.21, ptr %v.20, align 1
  %v.22 = getelementptr { { i32 }, {  }, i8, { i64, i8 }, i64 }, ptr %local.9, i32 0, i32 3
  %v.23 = getelementptr i8, ptr %v.22, i64 0
  %v.24 = getelementptr i8, ptr %v.23, i64 0
  %v.25 = call ptr @zil.fn.0(ptr %v.24)
  store ptr %v.25, ptr %local.11, align 8
  %v.26 = getelementptr { { i32 }, {  }, i8, { i64, i8 }, i64 }, ptr %local.9, i32 0, i32 4
  %v.27 = getelementptr i8, ptr %v.26, i64 0
  store ptr %v.27, ptr %local.12, align 8
  %v.28 = load ptr, ptr %local.11, align 8
  %v.29 = load ptr, ptr %local.11, align 8
  %v.30 = load i64, ptr %v.29, align 8
  %v.31 = load ptr, ptr %local.12, align 8
  %v.32 = load i64, ptr %v.31, align 8
  %tmp.7 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.30, i64 %v.32)
  %v.33 = extractvalue { i64, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i64, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  store i64 %v.33, ptr %v.28, align 8
  %v.34 = load ptr, ptr %local.12, align 8
  %v.35 = load ptr, ptr %local.12, align 8
  %v.36 = load i64, ptr %v.35, align 8
  %tmp.9 = getelementptr { { i32 }, {  }, i8, { i64, i8 }, i64 }, ptr %local.9, i32 0, i32 2
  %v.37 = load i8, ptr %tmp.9, align 1
  %v.38 = zext i8 %v.37 to i64
  %tmp.10 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.36, i64 %v.38)
  %v.39 = extractvalue { i64, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i64, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.3
checked.3:
  store i64 %v.39, ptr %v.34, align 8
  %v.40 = load ptr, ptr %local.11, align 8
  %v.41 = load i64, ptr %v.40, align 8
  %v.42 = add i64 0, 5000000011
  %tmp.12 = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %v.41, i64 %v.42)
  %v.43 = extractvalue { i64, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i64, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.4
checked.4:
  %tmp.14 = icmp slt i64 %v.43, -2147483648
  %tmp.15 = icmp sgt i64 %v.43, 2147483647
  %tmp.16 = or i1 %tmp.14, %tmp.15
  br i1 %tmp.16, label %zil.trap, label %checked.5
checked.5:
  %v.44 = trunc i64 %v.43 to i32
  %v.45 = load ptr, ptr %local.12, align 8
  %v.46 = load i64, ptr %v.45, align 8
  %v.47 = add i64 0, 18
  %tmp.17 = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %v.46, i64 %v.47)
  %v.48 = extractvalue { i64, i1 } %tmp.17, 0
  %tmp.18 = extractvalue { i64, i1 } %tmp.17, 1
  br i1 %tmp.18, label %zil.trap, label %checked.6
checked.6:
  %tmp.19 = icmp slt i64 %v.48, -2147483648
  %tmp.20 = icmp sgt i64 %v.48, 2147483647
  %tmp.21 = or i1 %tmp.19, %tmp.20
  br i1 %tmp.21, label %zil.trap, label %checked.7
checked.7:
  %v.49 = trunc i64 %v.48 to i32
  %tmp.22 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.44, i32 %v.49)
  %v.50 = extractvalue { i32, i1 } %tmp.22, 0
  %tmp.23 = extractvalue { i32, i1 } %tmp.22, 1
  br i1 %tmp.23, label %zil.trap, label %checked.8
checked.8:
  %tmp.24 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.24, label %package.drop.9, label %package.done.9
package.drop.9:
  store i1 false, ptr %resource.live.9, align 1
  br label %package.done.9
package.done.9:
  store i1 false, ptr %resource.live.9, align 1
  ret i32 %v.50
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
