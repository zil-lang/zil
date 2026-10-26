# 085：u16 与有限静态外部值的源面、类型和分析

父任务 B0；规范为 087 提案和 schema15。此切片不授予 Verify 能力，不报告 CT、ABI 机器行为或窗口运行完成。

## 实施范围

- 源面新增 u16 后缀、普通类型和保值转换；十进制、二/八/十六进制沿用原分隔符验证，整数推断域不把外部句柄当整数。
- `extern "C" fn(...) -> T` 保留为独立 Syntax/IR 回调类型。`c_null()`、`c_wstr_null()`、`c_wstr(literal)` 进入冻结的 Core Literal；字符串按标量转义解码后转为合法 UTF-16，追加唯一末尾 NUL，空串与 NULL 分开。
- `c_callback(symbol)` 只认静态普通函数，生成 ForeignFunctionRef。拒绝存储的普通函数指针、闭包、foreign、资源 teardown、receiver、保留 callable/return contract 及超出窄 C 签名的目标；不产生 CallIndirect。
- C 导入接受冻结的窄值集合；共享和可变地址分别按严格 POD 集合验证，所有地址仍须完整 noescape。直接或递归共享字段中的 callback 必须有 static_callbacks；标记也能单独描述反调先前注册 callback 的外部调用。
- 三条 Types 表达式入口、泛型/别名/模块/receiver 遍历和接口身份摘要接通新值。静态回调不复用普通 FunctionPointer 的约束形状。
- Analyze 对静态常量单位数、callback 签名和新增 foreign metadata 独立预检计费，ForeignFunctionRef 无数据来源操作数；沿用原初始化、借用、资源和清理分析。

## 限定验证

`crates/zil-driver/tests/impl085_frontend.rs` 包含 10 组真实 Parse→Expand→Types 源测试，部分同时进入 Analyze。覆盖正常 Win32 Class/MSG/PAINTSTRUCT 形状、u16 ATOM、UTF-16/NULL、静态 callback、闭包与泛型 Copy 转发、拒绝边界和 raw 预算。该文件可在集成后由 driver 测试入口运行；阶段尚未拼齐时用仅依赖上述前端 crate 的临时 Cargo harness 运行同一文件，不依赖宿主程序解释 ZIL。

真实跨模块 Win32 消费者在源码 checkpoint `e2ffd06ec7e4e32ba022ce7151e9a262f62513a4` 暴露 project normalizer 未遍历 Extern 签名的问题：私有 WindowClass/Message/Paint/Rect 保持未限定 Named，导致 Types 拒绝。新增两模块回归先复现同样诊断，再将 Extern 接入普通函数共用的归一和还原遍历；检查四种地址目标均解析为 Record、C symbol 保持原拼写，并保留不同模块同名私有类型的隔离。原始 Win32 失败 JSON 和最近回归失败均保存在证据目录。

受影响的 Syntax、Expand、Types、Analyze 四 crate 原有 114 项测试同时回归。原始日志和精确源码提交由 `evidence/impl/085` 收据记录；完整 Verify、CT、LLVM、Win64 与产品门交由对应施工及独立验收。

## Rust 四目标与边界

本切片缩短纯 ZIL 直接调用系统 API 的实际路径，保留显式 unsafe、独立 Verify 和受限外部契约；没有添加 C UI 宿主、RC、全局对象表或借用来源擦除。静态 C callback 与 readonly UTF-16 是可检查的有限前置，不等于通用 FFI、异步回调、动态容器、字符串库或完整 ZUI。性能改善未在本切片测量，也不把新语法或测试数量当作对 Rust 的成本优势。

## 非作者退回后的形成检查修复

独立审核在交付 `84d49be9bebc68448b6e9383b612bf2b0095393a` 发现：unused generic fn、alias 和 generic struct 可在擦除时隐藏确定非法的 C callback ABI/参数/结果类型。原候选不追记通过，原始失败由 `evidence/reviews/085` 保留。

修复在已有的借入 AST 预检中检查 callback 的固定形成规则；alias 解析完成后以无类型变量作用域复核，泛型函数签名与结构字段则递归按真实类型变量作用域复核，重复扫描计入既有预算。具体 bool、u8/u16、Unit 参数、c_wstr 结果、结构/地址参数和非 C ABI 不能随未使用声明消失；未解析的合法 T 仍待特化，实例化结果继续进入 Types/Verify。新增 17 个拒绝对照、6 个合法/待特化对照。

修复不增加 callback 签名内部 T 的结构实参匹配；即使显式写 `ignore::<i32>(c_callback(...))`，当前匹配器也仍明确返回 E-GENERIC-ARGUMENT。普通 generic alias 的原 Unsupported 范围不改变。具体 Wrapper<i32> 字段特化通过，Wrapper<bool> 拒绝。
