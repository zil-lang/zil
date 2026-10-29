; ZIL verified LLVM module
; verified-fingerprint: 71f0d980f2b0e92927d76e79a6bb326d2342b20a5fd1682d43a5576b8e4d8d1f
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
%zil_env_1 = type {  }
%zil_env_2 = type {  }
%zil_env_3 = type {  }
%zil_env_4 = type {  }
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
define internal void @zil.closure.drop.3(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.4(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca { i32, i32 }, align 4
  %local.3 = alloca %zil_env_0, align 1
  %local.4 = alloca %zil_env_1, align 1
  %local.5 = alloca %zil_env_2, align 1
  %local.6 = alloca i32, align 4
  %local.7 = alloca i32, align 4
  %local.8 = alloca [2 x i32], align 4
  %local.9 = alloca %zil_env_3, align 1
  %local.10 = alloca %zil_env_4, align 1
  %local.11 = alloca i32, align 4
  %local.12 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 20
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 22
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = load i32, ptr %local.1, align 4
%tmp.0 = insertvalue { i32, i32 } zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue { i32, i32 } %tmp.0, i32 %v.3, 1
  store { i32, i32 } %v.4, ptr %local.2, align 4
  %v.5 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.5, ptr %local.3, align 1
  %v.6 = freeze %zil_env_1 zeroinitializer
  store %zil_env_1 %v.6, ptr %local.4, align 1
  %v.7 = freeze %zil_env_2 zeroinitializer
  store %zil_env_2 %v.7, ptr %local.5, align 1
  %v.8 = load %zil_env_1, ptr %local.4, align 1
  %v.9 = getelementptr i8, ptr %local.2, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = call i32 @zil.fn.2(%zil_env_1 %v.8, ptr %v.11)
  %v.13 = load %zil_env_0, ptr %local.3, align 1
  %v.14 = load { i32, i32 }, ptr %local.2, align 4
  %v.15 = call i32 @zil.fn.1(%zil_env_0 %v.13, { i32, i32 } %v.14)
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.12, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.16, ptr %local.6, align 4
  %v.17 = load %zil_env_2, ptr %local.5, align 1
  %v.18 = getelementptr i8, ptr %local.2, i64 0
  %v.19 = getelementptr i8, ptr %v.18, i64 0
  %v.20 = getelementptr i8, ptr %v.19, i64 0
  %v.21 = call i32 @zil.fn.3(%zil_env_2 %v.17, ptr %v.20)
  store i32 %v.21, ptr %local.7, align 4
  %v.22 = add i32 0, 1
  %v.23 = add i32 0, 2
%tmp.3 = insertvalue [2 x i32] zeroinitializer, i32 %v.22, 0
%v.24 = insertvalue [2 x i32] %tmp.3, i32 %v.23, 1
  store [2 x i32] %v.24, ptr %local.8, align 4
  %v.25 = freeze %zil_env_3 zeroinitializer
  store %zil_env_3 %v.25, ptr %local.9, align 1
  %v.26 = freeze %zil_env_4 zeroinitializer
  store %zil_env_4 %v.26, ptr %local.10, align 1
  %v.27 = load %zil_env_3, ptr %local.9, align 1
  %v.28 = getelementptr i8, ptr %local.8, i64 0
  %v.29 = getelementptr i8, ptr %v.28, i64 0
  %v.30 = getelementptr i8, ptr %v.29, i64 0
  %v.31 = call i32 @zil.fn.4(%zil_env_3 %v.27, ptr %v.30)
  store i32 %v.31, ptr %local.11, align 4
  %v.32 = load %zil_env_4, ptr %local.10, align 1
  %v.33 = getelementptr i8, ptr %local.8, i64 0
  %v.34 = getelementptr i8, ptr %v.33, i64 0
  %v.35 = getelementptr i8, ptr %v.34, i64 0
  %v.36 = call i32 @zil.fn.5(%zil_env_4 %v.32, ptr %v.35)
  store i32 %v.36, ptr %local.12, align 4
  %v.37 = load i32, ptr %local.6, align 4
  %v.38 = load i32, ptr %local.7, align 4
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.37, i32 %v.38)
  %v.39 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  %v.40 = load i32, ptr %local.11, align 4
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.39, i32 %v.40)
  %v.41 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  %v.42 = load i32, ptr %local.12, align 4
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.41, i32 %v.42)
  %v.43 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  %v.44 = add i32 0, 111
  %tmp.10 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.43, i32 %v.44)
  %v.45 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.4
checked.4:
  ret i32 %v.45
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env, { i32, i32 } %arg.0) {
entry:
  %local.0 = alloca { i32, i32 }, align 4
  store { i32, i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i32, i32 }, ptr %local.0, align 4
%v.1 = extractvalue { i32, i32 } %v.0, 0
  %v.2 = load { i32, i32 }, ptr %local.0, align 4
%v.3 = extractvalue { i32, i32 } %v.2, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.4
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
  %v.1 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  %v.4 = load ptr, ptr %local.0, align 8
  %v.5 = getelementptr { i32, i32 }, ptr %v.4, i32 0, i32 1
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = load i32, ptr %v.6, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3(%zil_env_2 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 0
  %v.2 = load ptr, ptr %local.0, align 8
  %v.3 = getelementptr { i32, i32 }, ptr %v.2, i32 0, i32 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = load i32, ptr %v.4, align 4
  %v.6 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.7, ptr %v.1, align 4
  %v.8 = load ptr, ptr %local.0, align 8
  %v.9 = getelementptr { i32, i32 }, ptr %v.8, i32 0, i32 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = load i32, ptr %v.10, align 4
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4(%zil_env_3 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = add i32 0, 0
  %tmp.0 = sext i32 %v.1 to i64
  %tmp.1 = icmp ult i64 %tmp.0, 2
  br i1 %tmp.1, label %address.ok.0, label %zil.trap
address.ok.0:
  %v.2 = getelementptr [2 x i32], ptr %v.0, i64 0, i64 %tmp.0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = load i32, ptr %v.3, align 4
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = add i32 0, 1
  %tmp.2 = sext i32 %v.6 to i64
  %tmp.3 = icmp ult i64 %tmp.2, 2
  br i1 %tmp.3, label %address.ok.1, label %zil.trap
address.ok.1:
  %v.7 = getelementptr [2 x i32], ptr %v.5, i64 0, i64 %tmp.2
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = load i32, ptr %v.8, align 4
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(%zil_env_4 %env, ptr %arg.0) {
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
  %v.15 = load ptr, ptr %local.0, align 8
  %v.16 = add i32 0, 0
  %tmp.8 = sext i32 %v.16 to i64
  %tmp.9 = icmp ult i64 %tmp.8, 2
  br i1 %tmp.9, label %address.ok.4, label %zil.trap
address.ok.4:
  %v.17 = getelementptr [2 x i32], ptr %v.15, i64 0, i64 %tmp.8
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  %v.19 = load i32, ptr %v.18, align 4
  ret i32 %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
