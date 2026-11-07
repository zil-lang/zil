# ZIL 语言初稿开发预览

状态：**有限 profile，实验性开发预览**。受支持环境为 Rust 1.99.1、LLVM 23.1.4、Linux x86_64。精确受测源码、独审、实际执行计数及恢复范围见[联合验收记录](https://github.com/zil-lang/zil/blob/goal/records/native-language-composition.json)。本页列出有限能力与边界；完整语言、B0、ZUI 和 Rust 产品目标分别验收。

当前有限拥有组合包括普通 POD、九种标量和完整 Copy Tuple/Array 的 Store、普通拥有父形状与自包含捕获。借入 dyn 支持固定标量/Unit 槽和共享/独占接收者。具体范围与限制如下。

普通拥有 Package 可组合已支持的资源与多个外部可变借入叶，并保留真实来源、父贷款、槽替换后的状态和嵌套子 Package 清理；通用取走/补回与更广的异步组合继续保留边界。规则分别见[可变存储载体](../../spec/mutable-stored-holder.md)、[trait 合同](../../spec/trait-method-lending.md)和[affine Mutex](../../spec/affine-mutex.md)。[基础语言发布门](../../validation/language-release-gates.json)定义完整功能验收，不代表当前预览已达到1.0。

## 支持矩阵

| 能力簇 | 预览支持范围 | 验证范围 | 边界 |
| --- | --- | --- | --- |
| 拥有组合与动态派发 | 有限拥有父形状、捕获和递归 Package 具名模式；两提供者共享/独占借入 dyn 的标量及 Unit 槽 | 实际运行时选择、跨模块默认包装、字段写入/读回、完整合同、raw/Wire、缓存及双后端 O0/O2 | 拥有擦除存储、借入 dyn 结果、一般关联或泛型槽仍不开放；不能给未知效果空合同 |
| 普通集合、文本与 I/O | 完整 POD Tuple/Array/Record 和九种标量 Store；UTF8 String 查询/编辑、整数转换、排序、文件/流读写、StringMap、固定容量 POD HashMap 与名义 i64 Map/Set | 真实普通库、私有 Key 跨模块消费者、容量/分配失败返还责任、短写/EOF 与两后端 O0/O2 | 一般 owning K/V、自动增长泛型 HashMap 和完整 Unicode 排版未由有限库覆盖；Store 可变窗口保持整个实际 Store 排他 |
| trait 与泛型库 | 普通泛型 trait、关联类型/常量及字段/别名投影、共享逐调用 lending、静态关联函数、独立 K/V 与回调约束 | 定义期义务、真实 trait/provider 身份、私有消费者类型、完整 nominal/life 实参与双核/原生 | 更广 impl 宿主、owning 泛型/lending 及一般约束方程仍需各自完整证明 |
| 普通程序组合 | 选定真实跨模块程序中的集合更新、错误传播、明确资源责任与短借用；holder 函数组件通过直接 `fn(i32)->i32` 与普通 callback/Task/登记组件组合 | 固定源文件/输入/完整 stdout、退出状态、失败清理、合法及最近反例；精确提交与 O0/O2 结果见共同收据 | 地址、holder、拥有值、环境及间接定义边不跨组件；同函数内存储载体与异步/拥有操作混合仍待统一证明 |
| 可变借入存储 | 有限 Record/拥有 Package 的多个直接地址叶；槽替换、跨模块 helper 当前状态与嵌套 qualified 子 Package | 真实 Source、独立 raw/Wire、相交/生命周期反例、Clang/LLVM API O0/O2 与实际资源释放 | 通用 move_out/take-refill、可变捕获及跨异步载体运输尚未完整准入；拥有字段和借入资格分别守恒 |
| 借入 Task | 多个共享/独占输入、有限标量/Record/Tuple/Array/Slice referent、普通闭合结果；显式 Task<Result,Context> 的泛型 helper 转交 | PC0、挂起、取消/关闭、逐端口实际父贷款、未再使用 SSA 的持有、raw/Wire 与四机器配置 | 借用保留到真实 Destroy；借入结果、嵌套 owner/capture 运输、线程迁移与外部 ABI 仍未开放 |
| 线程与同步 | 结构化线程、实际 pthread Mutex<T>/affine Guard、拥有 Package/Tuple/Enum/Array payload、明确来源的多输入 helper；i64/u64/usize 原子及强 CAS | 实际共享线程、逐输入来源、顺序合法矩阵、双后端 O0/O2、malloc/init 失败与逐元素一次清理 | Guard 不自动获得线程迁移或跨挂起资格；资源结构须满足实际同步清理与 Transfer/Share 证明 |
| 登记对象状态合同 | 单借入 `AsyncRegistration<F>` helper 的逐结果三态合同、跨模块调用、真实 Pending 重试和静态 checked finish | 未用定义期检查、独立两核来源/旧结果/预算反例、真实外部队列和双后端 O0/O2 | 模板定义期仅直线真实原语/别名/返回；一般用户协议、多角色/投影及跨线程关闭未开放；旧运行时受检 finish 保留 |
| 项目使用 | 项目清单、路径依赖与锁、check/build/run/test、真实测试发现和失败状态 | 可复现依赖解析、每 importer 同名 helper、CT/缓存、Unit/Bool/I32 测试与最近失败反例 | 无集中注册表；空选择、失败编译、超时和失败测试不报 Pass |
| 交付与质量 | Source→Types→Analyze→独立 Verify→机器产物的有限 profile；CLI 区分 Success、SafeReject、Unsupported、ResourceLimit、InfraError | 源码和摘要绑定绑定源码的共同收据；完整默认/参考回归、两后端原生、失败清理及冷恢复分别记录，结果与冷恢复边界按精确源码记录 | 零命中、Unsupported、超时、工具失败与旧提交收据均非本次 Pass；正式 B0 和产品门维持原状态 |

## 快速开始

在具备 `rust-toolchain.toml` 指定 Rust、相应 LLVM/clang 与系统链接器的已支持环境中，从仓库根目录运行。LLVM 安装、环境变量和版本检查见 [LLVM 工具链](build.md#安装与构建)；先 `cargo fetch --locked` 备妥依赖，再使用离线构建：

```sh
cargo build --offline --locked -p zil-cli
./target/debug/zil version
./target/debug/zil check examples/mainline/stream/main.zil --json
mkdir -p out
./target/debug/zil build examples/mainline/stream/main.zil -o out/stream --emit-dir out/stream-stages
printf '10 20 -3\n' | ./out/stream
```

该已存在的普通多模块样例预期输出为 `3 0 45`（含换行），退出 0；冻结验收已在远端重新拉取的同一源码与所标环境执行。组合样例在 `examples/mainline/draft_foundations/`，结合实际 std.store、trait 静态工厂、可变 helper 与显式失败处理。开发运行命令如下（冻结恢复实跑使用 release CLI）：

```sh
./target/debug/zil run examples/mainline/draft_foundations/main.zil --entry main
# 完整返回值应为 42
```

## 未支持及环境限制

- 拥有 `dyn`、一般借入枚举/结果、长期借用与资源宿主 lending、泛型拥有集合、跨线程登记关闭及其更广组合仍有未闭合范围。父能力不能因某个有限子项通过而视为完成。
- GPU 首帧、字体、OS IME、完整 ZUI 与原生 Windows 执行不包含在本有限语言预览。目标/后端只按本次收据中真实执行的组合列出；Win64 交叉构建或 Wine 不等于原生 Windows 验证。
- `validation/product-gates.json` 中零 safe UB、同语义 Rust 运行/内存与编译/维护成本条件、至少一项可重复净突破及发布前提原样有效。目前不得宣称这些产品条件达成。B0、完整语言与 ZUI 终态验收继续独立进行。

本预览依据 `_zil_design.md` §20、`validation/product-gates.json`、`goal:workflow.md` 与实时 `goal:goal.md`。源码、各门结果及恢复范围绑定共同收据；远端恢复复用已准备的固定工具链和 Cargo 缓存，不代表空机器安装或旧历史重新验收。
