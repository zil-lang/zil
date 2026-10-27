# 171：有限资源的真实共享头地址

状态：施工提案；基线 d9f8f9469cf58b4d7dd4a1d2ac966bd1dc0ee3e0。

依据 `_zil_design.md` ZIL-MODES §2.2、ZIL-ACCESS、ZIL-ORIGIN、ZIL-CLEANUP、ZIL-LOWERING。
本提案实施已规定的真实地址窗口，不改变资源默认 Desc、give 或清理合同。

## 首片合同

- `&R` / trait `self: &Self` 在 R 为有限 Copy payload 用户 resource 时，解析为 `Address<shared, Managed<Record<R>>>`；机器指针直接引用原 Managed 局部内联头。
- 复用 BorrowLocal、ReborrowAddress、AddressField；字段投影只能读既有有限 Copy payload。地址本身无清理责任，不能 ReadAddress 整头或 WriteAddress 覆写整头。
- 源调用以实际 Managed 局部建立窗口；资源 Desc、Package 内资源字段、ByteBuffer 密封头、可变头地址、头地址返回和头地址捕获在本片明确拒绝。
- 同步普通参数、trait/inherent 共享方法与跨模块调用传递真实头地址。地址使用期间同源 give、清理、替换和 Desc 写入必须独立拒绝；最后一次使用后允许既有资源消费。
- 参数名不证明不交；同一个头经 Desc 与 Address 两种入口传入会按实际来源检查。若尚不能以有序合同证明安全，保守拒绝冲突，不能默认独立。

## 实施与公共接口

不增加 opcode/Wire 字段。公共 Type 只增加 exact shared resource-head 地址结构分类 helper；不将任意 Managed 头加入一般 `is_address_pointee`。
Types 解析名义模式和实际局部；Analyze 从 CFG 建立候选真实来源与资源写效果；Verify 重新核验资源名义表、地址原语、局部活性、Head/Desc 根及相交权限，不调用 Types/Analyze 证明。
Lower/LLVM 允许已验证形状，沿用实际 `%local` 指针与 Managed payload 布局，不增加 Box/RC/noalias。

## 最近反例与证据

保留 159 原 `trait Read { fn read(self: &Self) -> i32; } resource C ...` 声明探针；增加真实调用、普通 &C 参数、多模块及 O0/O2。
源码负例包括 Desc 借成头、&mut、头地址返回/捕获、给出/替换后再读、共享期间 Desc 写、same-layout 不同名义资源、Drop 普通调用。
raw 与 wire 分别篡改头复制/覆写、名义表、Mutable 标记、Hold 候选/旧槽重生；所有拒绝核对阶段与理由。
保留构建和运行原始命令、提交/输入/工具链身份、真实结果；不把测试数量称为产品安全或 Rust 成本结论。
