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

## 限定交付与真实结果

生产/测试冻结 `6a0b10cfec5f88d4b4bfdefe42d8d24ce689760a`，包含 root 授权的
规范同步 `dde8be30124f9fafa0d5cb7ed7c281c9fcecf798`；之后本片只追加本文结果。
原334文件 SHA256 仍为
`b7840ba2c00ccd1df99cc3389bc6363e25a49b764311c8c14c9ac07a76628696`。
实现没有修改默认体注入、两核、公共接口或后端；默认 helper 仍沿既有普通函数链，
实际 Typed 含 `FunctionRef` 与 `CallIndirect`，LLVM 以 `ptr` 返回目标并进行
`call i32 %v.4(i32 %v.5)`。该 Copy code pointer 没有新环境或清理责任。

在前述环境下实际执行：

```sh
cargo test -p zil-driver --features llvm-api --test impl162_trait_bounds trait336_ -- --nocapture
cargo test -p zil-driver --features llvm-api --test impl162_trait_bounds -- --nocapture
cargo clippy -p zil-expand --lib -- -D warnings
cargo clippy -p zil-driver --features llvm-api --test impl162_trait_bounds -- -D warnings
rustfmt --edition 2024 crates/zil-expand/src/generic_check.rs crates/zil-driver/tests/impl162_trait_bounds.rs
git diff --check
```

最终新增过滤组为 `8 passed / 0 failed / 38 filtered`；整个既有目标为
`46 passed / 0 failed / 0 ignored / 0 filtered`。两项 Clippy、fmt 与 diff 检查通过。
三个真实源场景各经过 Clang/API 的 O0/O2，共12个机器组全部返回0；实际 backend、
API builder object 与无 fallback 由既有 harness 断言。重跑不累加为新覆盖。
场景包括原334源、trait 定义模块私有宽整数双参数函数与 impl 同名劫持控制、真实
fnptr 参数转交及局部 shadow。完整参数/结果/arity、同布局名义、有序 phantom、
trait 符号 R、外部同拼写 Copy/非Copy R、未用和覆盖后的坏默认体均被源测试核回。
同 session 修改真实全局函数参数签名会拒绝，恢复原源后再次成功，与 fresh 相同。

新请求精确边界为 `work=2310 / nodes=552 / functions=4`；每项 N 成功、N−1
ResourceLimit、cold/warm/fresh 一致，active query 均清空。Copy 资格失败的
Unsupported 转换保留 ResourceLimit 类别。原323控制仍为 `3797 / 853 / 5`，
原330控制仍为 `10383 / 1766 / 9`；没有提高默认额度。

首轮真实结果是 `5 passed / 2 failed / 38 filtered`，保留而不计为通过：一项测试把
合法外部声明写成不支持的 extern 大括号形式，实际 Parse/E-PARSE-EXPECTED；修正为
既有 `extern "C" fn` 后才测试 foreign 函数值的显式 Unsupported。另一项过度期待
default 中 `hidden.identity` 的 E-MODULE-PRIVATE；当前 qualified Member 默认体仍
在既有符号路径报 Expand/E-GENERIC-ARGUMENT `unknown value hidden`。本片没有
开放这一路径，回归精确保留原拒绝，并以普通函数值控制另核 E-MODULE-PRIVATE。
这些修正只改夹具/断言，生产未因首轮失败改动。qualified 默认函数值仍是后续缺口。

root 另授权同 target 的窄 CLI 构建 `cargo build -p zil-cli`，build start/end HEAD
均为上述 `6a0b10cfe`，两端工作树干净；默认 llvm-api。二进制
`/workspace/.zil-env/target336/debug/zil` SHA256 为
`ead3a875ee708673f6fd31ca1af2b12f67e36e20ddf6460ba2a1f59277703fcd`，已供独立审核
只读复制；整个 target 257MiB。工具链为 rustc/cargo 1.99.1、LLVM clang 23.1.4，
Linux x86_64。该 CLI 对原334源实际给出 Success/verify、Verify1；新 fingerprint 为
`81cf897eafe2cfb284a47a76dfde27b4b0a5a836a37fdc05c6cc7fc2de8183c8`，与旧 B 的
fingerprint 分列，原源码字节不变。

原334/API/O0 的实际阶段快照（临时目录
`/workspace/.zil-env/tmp-batch336/336/bounds162-548134-68`）SHA256：

| 产物 | SHA256 |
| --- | --- |
| typed.txt | c46724e838d2668513298665d4c9585cb50fbdc5fc73cbbdb481ffa8517b6164 |
| verified.txt | 929ec69f78e2cd54196468405283e0ff965a6e4d9a2a0d722cf8763e250b039e |
| program.ll | c7999fcee71c25502e3ddeff39d7a0af23956a4a1aba40a6d3a802fa24669a2f |

临时 raw 产物不提交，也不以这些路径保证跨机器可用；交付保留源码、可重跑测试和本文
精简结果。正式共同全量、精确非作者裁决、主树状态与远端备份由 root 管理。

| Rust目标 | 本片实证与限制 |
| --- | --- |
| 同类安全 | 定义期完整实际签名和独立 Copy 资格、实际两核/native 贯通；完整语言安全未完成。 |
| 运行/内存 | 保持薄指针与普通静态 helper，无新隐式分配；未测同语义 Rust 时间/RSS。 |
| 表达/维护 | 原合法默认体全局函数值可直接返回，私有模块与 shadow 组合可用；框架维护收益未测。 |
| 编译成本 | 新请求精确 N/N−1/fresh 与原控制保持；没有 Rust 编译性能比较。 |
| 至少一项突破 | 原 KnownIncomplete 源到双后端机器执行是有限能力增量；产品级四目标合取与相对 Rust 净突破未建立。 |

缺结果标注、generic/foreign/借用或拥有函数值、qualified 默认函数值及其它完整 trait
能力继续各按真实边界保留；B0、完整语言/ZUI及既有 product-gates 不因本片升级。
