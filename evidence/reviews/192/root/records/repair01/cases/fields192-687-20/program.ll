; ZIL verified LLVM module
; verified-fingerprint: 07a0890f919fd59157de2905fe46a402c6ad61be0665a4bba948e1c181388666
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca { i32, i32 }, align 4
  %local.5 = alloca [2 x i32], align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { { i32 }, { i32, i32 }, [2 x i32] }, align 4
  %local.8 = alloca ptr, align 8
  %local.9 = alloca i32, align 4
  %local.10 = alloca ptr, align 8
  %local.11 = alloca i32, align 4
  %local.12 = alloca ptr, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.6, align 1
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i32 0, 20
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = add i32 0, 1
  store i32 %v.4, ptr %local.3, align 4
  %v.5 = load i32, ptr %local.2, align 4
  %v.6 = load i32, ptr %local.3, align 4
%tmp.0 = insertvalue { i32, i32 } zeroinitializer, i32 %v.5, 0
%v.7 = insertvalue { i32, i32 } %tmp.0, i32 %v.6, 1
  store { i32, i32 } %v.7, ptr %local.4, align 4
  %v.8 = add i32 0, 30
  %v.9 = add i32 0, 2
%tmp.1 = insertvalue [2 x i32] zeroinitializer, i32 %v.8, 0
%v.10 = insertvalue [2 x i32] %tmp.1, i32 %v.9, 1
  store [2 x i32] %v.10, ptr %local.5, align 4
  %v.11 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.11, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.12 = load { i32, i32 }, ptr %local.4, align 4
  %v.13 = load [2 x i32], ptr %local.5, align 4
  %v.14 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
%tmp.2 = insertvalue { { i32 }, { i32, i32 }, [2 x i32] } zeroinitializer, { i32 } %v.14, 0
%tmp.3 = insertvalue { { i32 }, { i32, i32 }, [2 x i32] } %tmp.2, { i32, i32 } %v.12, 1
%v.15 = insertvalue { { i32 }, { i32, i32 }, [2 x i32] } %tmp.3, [2 x i32] %v.13, 2
  %tmp.4 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.4, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.6, align 1
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.5 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.5, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.7, align 1
  store { { i32 }, { i32, i32 }, [2 x i32] } %v.15, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.16 = getelementptr { { i32 }, { i32, i32 }, [2 x i32] }, ptr %local.7, i32 0, i32 1
  %v.17 = getelementptr i8, ptr %v.16, i64 0
  store ptr %v.17, ptr %local.8, align 8
  %v.18 = load ptr, ptr %local.8, align 8
  %v.19 = getelementptr { i32, i32 }, ptr %v.18, i32 0, i32 0
  %v.20 = load ptr, ptr %local.8, align 8
  %v.21 = getelementptr { i32, i32 }, ptr %v.20, i32 0, i32 0
  %v.22 = getelementptr i8, ptr %v.21, i64 0
  %v.23 = load i32, ptr %v.22, align 4
  %v.24 = load ptr, ptr %local.8, align 8
  %v.25 = getelementptr { i32, i32 }, ptr %v.24, i32 0, i32 1
  %v.26 = getelementptr i8, ptr %v.25, i64 0
  %v.27 = load i32, ptr %v.26, align 4
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.23, i32 %v.27)
  %v.28 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.28, ptr %v.19, align 4
  %v.29 = load ptr, ptr %local.8, align 8
  %v.30 = getelementptr { i32, i32 }, ptr %v.29, i32 0, i32 0
  %v.31 = getelementptr i8, ptr %v.30, i64 0
  %v.32 = load i32, ptr %v.31, align 4
  store i32 %v.32, ptr %local.9, align 4
  %v.33 = getelementptr { { i32 }, { i32, i32 }, [2 x i32] }, ptr %local.7, i32 0, i32 2
  %v.34 = getelementptr i8, ptr %v.33, i64 0
  store ptr %v.34, ptr %local.10, align 8
  %v.35 = add i32 0, 0
  store i32 %v.35, ptr %local.11, align 4
  %v.36 = load ptr, ptr %local.10, align 8
  %v.37 = load i32, ptr %local.11, align 4
  %tmp.8 = sext i32 %v.37 to i64
  %tmp.9 = icmp ult i64 %tmp.8, 2
  br i1 %tmp.9, label %address.ok.3, label %zil.trap
address.ok.3:
  %v.38 = getelementptr [2 x i32], ptr %v.36, i64 0, i64 %tmp.8
  %v.39 = load ptr, ptr %local.10, align 8
  %v.40 = load i32, ptr %local.11, align 4
  %tmp.10 = sext i32 %v.40 to i64
  %tmp.11 = icmp ult i64 %tmp.10, 2
  br i1 %tmp.11, label %address.ok.4, label %zil.trap
address.ok.4:
  %v.41 = getelementptr [2 x i32], ptr %v.39, i64 0, i64 %tmp.10
  %v.42 = getelementptr i8, ptr %v.41, i64 0
  %v.43 = load i32, ptr %v.42, align 4
  %v.44 = load ptr, ptr %local.10, align 8
  %v.45 = add i32 0, 1
  %tmp.12 = sext i32 %v.45 to i64
  %tmp.13 = icmp ult i64 %tmp.12, 2
  br i1 %tmp.13, label %address.ok.5, label %zil.trap
address.ok.5:
  %v.46 = getelementptr [2 x i32], ptr %v.44, i64 0, i64 %tmp.12
  %v.47 = getelementptr i8, ptr %v.46, i64 0
  %v.48 = load i32, ptr %v.47, align 4
  %tmp.14 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.43, i32 %v.48)
  %v.49 = extractvalue { i32, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i32, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.6
checked.6:
  store i32 %v.49, ptr %v.38, align 4
  %v.50 = getelementptr { { i32 }, { i32, i32 }, [2 x i32] }, ptr %local.7, i32 0, i32 1
  %v.51 = getelementptr { i32, i32 }, ptr %v.50, i32 0, i32 1
  %v.52 = getelementptr i8, ptr %v.51, i64 0
  store ptr %v.52, ptr %local.12, align 8
  %v.53 = load i32, ptr %local.9, align 4
  %v.54 = load ptr, ptr %local.10, align 8
  %v.55 = add i32 0, 0
  %tmp.16 = sext i32 %v.55 to i64
  %tmp.17 = icmp ult i64 %tmp.16, 2
  br i1 %tmp.17, label %address.ok.7, label %zil.trap
address.ok.7:
  %v.56 = getelementptr [2 x i32], ptr %v.54, i64 0, i64 %tmp.16
  %v.57 = getelementptr i8, ptr %v.56, i64 0
  %v.58 = load i32, ptr %v.57, align 4
  %tmp.18 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.53, i32 %v.58)
  %v.59 = extractvalue { i32, i1 } %tmp.18, 0
  %tmp.19 = extractvalue { i32, i1 } %tmp.18, 1
  br i1 %tmp.19, label %zil.trap, label %checked.8
checked.8:
  %v.60 = load ptr, ptr %local.12, align 8
  %v.61 = load i32, ptr %v.60, align 4
  %tmp.20 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.59, i32 %v.61)
  %v.62 = extractvalue { i32, i1 } %tmp.20, 0
  %tmp.21 = extractvalue { i32, i1 } %tmp.20, 1
  br i1 %tmp.21, label %zil.trap, label %checked.9
checked.9:
  %v.63 = add i32 0, 54
  %tmp.22 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.62, i32 %v.63)
  %v.64 = extractvalue { i32, i1 } %tmp.22, 0
  %tmp.23 = extractvalue { i32, i1 } %tmp.22, 1
  br i1 %tmp.23, label %zil.trap, label %checked.10
checked.10:
  %tmp.24 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.24, label %package.drop.11, label %package.done.11
package.drop.11:
  store i1 false, ptr %resource.live.7, align 1
  br label %package.done.11
package.done.11:
  store i1 false, ptr %resource.live.7, align 1
  ret i32 %v.64
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
