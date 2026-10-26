# 086：静态外部值、C record 与回调的独立核验

2026-10-25。父任务 B0；公共 schema15 基线 39dfb6aad1f0d1c38213cbd83901ef01736d98fc。本文先于本单实现；接受性只落实 `_zil_design.md`、`spec/interfaces.json` 与已冻结的 087，不扩展外部合同，也不替代独立审核或正式产品门。

## 实施范围

- U16 与 U8 同为普通整数，受检保值转换；两个 CT 入口均可使用。
- ForeignHandle、CWideString、ForeignCallback 仅在完整受支持的 x64 Linux GNU / Win64 MSVC 目标元组中开放；遍历所有嵌套类型与资源载荷，不凭顶层类型遗漏新权限。
- 独立检查静态 UTF16 常量的唯一末尾 NUL、完整代理项配对；原始单元在散列和遍历前计费。不允许 foreign 返回或可变外部载荷制造静态字符串/回调。
- 独立从 ForeignFunctionRef 的真实函数定义重建受限 C 签名，拒绝 foreign、环境、closure entry、teardown 及保留 callable 合同；不把普通 FunctionPointer 或 CallIndirect 当作外部回调权限。
- C 声明区分共享静态外借与可变全位有效载荷；按完整结构比较借入参数，逐节点计费。callback 直接参数或共享载荷要求 static_callbacks，旧 false 合同仍无回调/重入。借入存储的原有初始化、独占、活性、noescape 与 lexical unsafe 核不变。
- 仅按地址传递的非空数组/record 使用固定字段序与自然布局；独立检查布局算术，不支持按值 aggregate、packing、union 或 bitfield。padding 不是字段数据。
- 新外部类型与常量在所有 CT 入口独立拒绝，不能由共享 Copy 类型允许集意外授予 CT 权限。

## 核验与证据

限定运行 zil-verify crate，提交 raw/wire 正例、最近篡改反例、UTF16/target/ABI/CT/借用边界、wire 截断与预算 N/N−1 结果。此处手填 IR 仅是独立信任边界证据；真实 ZIL→机器的布局、窄返回、窗口输入/绘制/关闭由 085/087 集成提供，不将 raw 用例或测试数量当作 GUI 完成。

独立 Verify 不调用 Types/Analyze 求证函数。Managed 包装的是可核验的释放责任流；opaque token 的实际 OS 活性、唯一归属及 C 声明真实性仍是外部 unsafe 合同。没有新增 RC、Box、世代句柄或全局对象表。
