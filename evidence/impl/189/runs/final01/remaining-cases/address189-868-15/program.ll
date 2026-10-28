; ZIL verified LLVM module
; verified-fingerprint: fe2f52f46762d8dcbcfad5f5d81dfec3fadd175595a93efc601686aafc01bd44
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
%zil_env_1 = type {  }
%zil_env_2 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.2(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i64, align 8
  %local.2 = alloca { i32, i64 }, align 8
  %local.3 = alloca %zil_env_0, align 1
  %local.4 = alloca %zil_env_1, align 1
  %local.5 = alloca i64, align 8
  %local.6 = alloca [2 x i32], align 4
  %local.7 = alloca %zil_env_2, align 1
  %index.slot.33 = alloca [2 x i32], align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 20
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i64 0, 22
  store i64 %v.1, ptr %local.1, align 8
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = load i64, ptr %local.1, align 8
%tmp.0 = insertvalue { i32, i64 } zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue { i32, i64 } %tmp.0, i64 %v.3, 1
  store { i32, i64 } %v.4, ptr %local.2, align 8
  %v.5 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.5, ptr %local.3, align 1
  %v.6 = freeze %zil_env_1 zeroinitializer
  store %zil_env_1 %v.6, ptr %local.4, align 1
  %v.7 = load %zil_env_0, ptr %local.3, align 1
  %v.8 = getelementptr i8, ptr %local.2, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = call i64 @zil.fn.1(%zil_env_0 %v.7, ptr %v.10)
  store i64 %v.11, ptr %local.5, align 8
  %v.12 = load %zil_env_1, ptr %local.4, align 1
  %v.13 = getelementptr i8, ptr %local.2, i64 0
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  call void @zil.fn.2(%zil_env_1 %v.12, ptr %v.15)
  %v.17 = add i32 0, 1
  %v.18 = add i32 0, 2
%tmp.1 = insertvalue [2 x i32] zeroinitializer, i32 %v.17, 0
%v.19 = insertvalue [2 x i32] %tmp.1, i32 %v.18, 1
  store [2 x i32] %v.19, ptr %local.6, align 4
  %v.20 = freeze %zil_env_2 zeroinitializer
  store %zil_env_2 %v.20, ptr %local.7, align 1
  %v.21 = load %zil_env_2, ptr %local.7, align 1
  %v.22 = getelementptr i8, ptr %local.6, i64 0
  %v.23 = getelementptr i8, ptr %v.22, i64 0
  %v.24 = getelementptr i8, ptr %v.23, i64 0
  call void @zil.fn.3(%zil_env_2 %v.21, ptr %v.24)
  %v.26 = load i64, ptr %local.5, align 8
  %tmp.2 = icmp slt i64 %v.26, -2147483648
  %tmp.3 = icmp sgt i64 %v.26, 2147483647
  %tmp.4 = or i1 %tmp.2, %tmp.3
  br i1 %tmp.4, label %zil.trap, label %checked.0
checked.0:
  %v.27 = trunc i64 %v.26 to i32
  %v.28 = load { i32, i64 }, ptr %local.2, align 8
%v.29 = extractvalue { i32, i64 } %v.28, 0
  %tmp.5 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.27, i32 %v.29)
  %v.30 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  %v.31 = load [2 x i32], ptr %local.6, align 4
  %v.32 = add i32 0, 0
  %tmp.7 = sext i32 %v.32 to i64
  %tmp.8 = icmp ult i64 %tmp.7, 2
  br i1 %tmp.8, label %index.ok.2, label %zil.trap
index.ok.2:
  store [2 x i32] %v.31, ptr %index.slot.33, align 4
  %tmp.9 = getelementptr [2 x i32], ptr %index.slot.33, i64 0, i64 %tmp.7
  %v.33 = load i32, ptr %tmp.9, align 4
  %tmp.10 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.30, i32 %v.33)
  %v.34 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.3
checked.3:
  %v.35 = add i32 0, 66
  %tmp.12 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.34, i32 %v.35)
  %v.36 = extractvalue { i32, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i32, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.4
checked.4:
  ret i32 %v.36
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.1(%zil_env_0 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32, i64 }, ptr %v.0, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  %v.4 = sext i32 %v.3 to i64
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = getelementptr { i32, i64 }, ptr %v.5, i32 0, i32 1
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = load i64, ptr %v.7, align 8
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.4, i64 %v.8)
  %v.9 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i64 %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.2(%zil_env_1 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32, i64 }, ptr %v.0, i32 0, i32 0
  %v.2 = load ptr, ptr %local.0, align 8
  %v.3 = getelementptr { i32, i64 }, ptr %v.2, i32 0, i32 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = load i32, ptr %v.4, align 4
  %v.6 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.7, ptr %v.1, align 4
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.3(%zil_env_2 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = load i32, ptr %local.1, align 4
  %tmp.0 = sext i32 %v.2 to i64
  %tmp.1 = icmp ult i64 %tmp.0, 2
  br i1 %tmp.1, label %address.ok.0, label %zil.trap
address.ok.0:
  %v.3 = getelementptr [2 x i32], ptr %v.1, i64 0, i64 %tmp.0
  %v.4 = load ptr, ptr %local.0, align 8
  %v.5 = load i32, ptr %local.1, align 4
  %tmp.2 = sext i32 %v.5 to i64
  %tmp.3 = icmp ult i64 %tmp.2, 2
  br i1 %tmp.3, label %address.ok.1, label %zil.trap
address.ok.1:
  %v.6 = getelementptr [2 x i32], ptr %v.4, i64 0, i64 %tmp.2
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = load i32, ptr %v.7, align 4
  %v.9 = load ptr, ptr %local.0, align 8
  %v.10 = add i32 0, 1
  %tmp.4 = sext i32 %v.10 to i64
  %tmp.5 = icmp ult i64 %tmp.4, 2
  br i1 %tmp.5, label %address.ok.2, label %zil.trap
address.ok.2:
  %v.11 = getelementptr [2 x i32], ptr %v.9, i64 0, i64 %tmp.4
  %v.12 = getelementptr i8, ptr %v.11, i64 0
  %v.13 = load i32, ptr %v.12, align 4
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.8, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  store i32 %v.14, ptr %v.3, align 4
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
