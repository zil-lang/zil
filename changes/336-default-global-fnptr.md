# 336 默认trait体中的真实静态函数值

基线 `e5a4d51d844e29c4f4f5184eaff5704b1b27dd24`；施工树从 root 冻结的
`c4da43441b03661e61d0087000a13dbbe7a2a75a` 建立。主树 `.task/impl/336.md`
与 `_zil_design.md` §18.1 profile336 为权威，不改变公共 IR/Wire、ABI、默认预算或产品门。

## 原始缺口与接受性

原文件 `tests/language-readiness/followup334/default-global-fnptr-known-incomplete.zil`
按原字节保留。加载 `/workspace/.zil-env/env.sh` 后，既有 B CLI
`/workspace/.zil-env/target-mainline-release/release/zil check <原文件> --json`
实际得到 `SafeReject/expand/E-GENERIC-ARGUMENT`，消息为
`unknown value $module0_increment in generic body`，Verify0；输入 fingerprint 为
`d3f2a690d8450d700802c9f712961bc4b5f8a30a14403170bf46096d36fd731b`。
这是合法待实现表达式，不能登记为永久负控；默认体转交已声明 fnptr 参数的旧能力保留。

仅在默认方法的 supplied-bound 符号检查上下文内，名字先查真实局部绑定；没有局部时，
才查既有词法正规化后的普通函数索引。薄指针类型必须从该函数完整实际参数和明确结果
标注产生，不从 expected 类型复制。无环境、非泛型、无额外合同且无 give 参数的普通
函数方可进入；缺明确结果、泛型、foreign、借用/拥有等未贯通形状显式 Unsupported。
参数和结果在空变量/空 bound 上下文独立核普通 Copy 资格，调用方符号 R 不能替外部
同拼写名义类型补证。完整类型比较保留名义、phantom、有序实参和符号 trait 参数身份。
未使用或被覆盖的默认体同样定义期检查。

复用已有项目名字正规化，trait 定义模块中的私有函数保持原目标；impl 模块同名项不能
劫持，模块外私有访问继续拒绝。不新增运行时字典、隐式 Box/RC/锁或环境载体。

## 写集与验证计划

生产只改 `crates/zil-expand/src/generic_check.rs` 的默认上下文 Name 分支及私有 helper。
限定既有 `impl162_trait_bounds` 测试目标，覆盖原334源、模块词法目标、局部 fnptr 转交、
完整签名/名义/phantom/符号身份、未用/覆盖后的错误体与明确 Unsupported 边界。
少量机器消费者复用 Clang/API 各 O0/O2，断言无 fallback；不跑正式全量。
新名字访问、参数扫描、Copy 检查和类型克隆先费后长，ResourceLimit 原类别透传。
实际源码请求 work/nodes/functions 的 N、N−1 与 cold/warm/fresh 验证不提高默认限额。

环境限定为 `CARGO_TARGET_DIR=/workspace/.zil-env/target336`、jobs1、debug0、incremental0，
临时输出 `/workspace/.zil-env/tmp-batch336/336`；不新增其它 Cargo target，不提交 raw 产物。
实际结果、失败与 Rust 四目标反思在限定验证后追加。
