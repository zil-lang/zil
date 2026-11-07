# ZIL 语言初稿开发预览

状态：**有限 profile，实验性开发预览**。受支持环境为 Rust 1.99.1、LLVM 23.1.4、Linux x86_64。精确受测源码、独审、实际执行计数及恢复范围见[联合验收记录](https://github.com/zil-lang/zil/blob/goal/records/native-language-composition.json)。本页列出有限能力与边界；完整语言、B0、ZUI 和 Rust 产品目标分别验收。

当前有限拥有组合包括普通 POD/九种标量 Store 进入 Package、Tuple、Enum、ErrorUnion，以及自包含 Fn/FnMut/FnOnce 捕获；共享借入 dyn 支持两个 Copy struct 提供者的固定标量方法槽。具体限制与其它支持能力如下。

新增有限组合包括单直接可变地址字段的名义 Record holder，以及普通 impl 关联类型等式的显式约束。holder 的子重借保留真实目标及载体父链；impl 约束在定义期按自身符号前提证明，保持 trait 与 impl 的独立 binder。边界分别见[可变存储载体](../../spec/mutable-stored-holder.md)和[trait 合同](../../spec/trait-method-lending.md)，精确源码验收索引维护于[协调分支](https://github.com/zil-lang/zil/tree/goal/records)。

## 支持矩阵

| 能力簇 | 预览支持范围 | 已验收证据 | 边界 |
| --- | --- | --- | --- |
| 有限拥有组合与共享动态派发 | 普通 POD/九种标量 Store 的有限拥有父形状和自包含捕获；两提供者的共享借入 dyn 固定标量槽 | 精确源码的 Source、独立核、Clang/LLVM API O0/O2、共同门及冷恢复见绑定源码的收据 | Unit dyn Source Unsupported；无 owning/mut dyn、擦除拥有存储或一般 lending |
| 普通集合与文本 | 真实 `std.store` 的有限 POD Record 和九种基础标量集合、容量管理、push/pop、共享与独占元素窗口；现有 UTF-8 字面量/只读入口和显式 String | 普通 POD Store 和文本已接受收据；原地修改与直接标量 Store 同批验收的源码、raw/Wire、两核、Clang/LLVMAPI O0/O2、分配失败与近邻拒绝 | 可变窗口须保持整 Store 排他，未知不同下标不能推定不交；一般拥有/异构容器、完整 Unicode/格式化另列 |
| trait 与借入迭代 | 现有普通泛型 trait、关联 Item、共享逐调用 lending；类型限定的静态关联函数及符号泛型宿主已通过绑定源码的验收 | 普通 trait lending 已接受收据；静态关联函数 具体/符号宿主源码、独立两核及机器结果 | 一般关联投影、资源宿主 lending 不随该历史片开放；该验收仅有限共享借入 dyn |
| 普通程序组合 | 选定真实跨模块程序中的集合更新、错误传播、明确资源责任与短借用；holder 函数组件通过直接 `fn(i32)->i32` 与普通 callback/Task/登记组件组合 | 固定源文件/输入/完整 stdout、退出状态、失败清理、合法及最近反例；精确提交与 O0/O2 结果见共同收据 | 地址、holder、拥有值、环境及间接定义边不跨组件；同函数内存储载体与异步/拥有操作混合仍待统一证明 |
| 可变借入存储 | 单直接可变地址字段的名义 Record holder；跨模块构造、helper 子重借与实际写入 | 真实 Source、独立 raw/Wire 反例、Clang/LLVM API O0/O2；借用目标和载体父链分别核验 | 不含混合拥有 Package、槽替换/取走、多可变叶或可变捕获；旧共享多叶行为保留 |
| 借入 Task | 单共享 `&i32` 输入、`Task<i32>` 结果；创建即保存真实指针，工厂及跨模块返回保留隐藏关系 | Source、独立两核 raw/Wire、PC0/挂起/取消/原生执行及最近反例 | 保守保持借用至成功 Destroy 或同步 Drop；不开放借入 Task 作为函数输入、其它结果族、捕获/聚合运输、可变 port、线程迁移或外部 ABI |
| 登记对象状态合同 | 单借入 `AsyncRegistration<F>` helper 的逐结果三态合同、跨模块调用、真实 Pending 重试和静态 checked finish | 未用定义期检查、独立两核来源/旧结果/预算反例、真实外部队列和双后端 O0/O2 | 模板定义期仅直线真实原语/别名/返回；一般用户协议、多角色/投影及跨线程关闭未开放；旧运行时受检 finish 保留 |
| 交付与质量 | Source→Types→Analyze→独立 Verify→机器产物的有限 profile；CLI 区分 Success、SafeReject、Unsupported、ResourceLimit、InfraError | 源码和摘要绑定绑定源码的共同收据；完整默认/参考回归、两后端原生、失败清理及冷恢复分别记录，全部通过，冷恢复边界单独记录 | 零命中、Unsupported、超时、工具失败与旧提交收据均非本次 Pass；正式 B0 和产品门维持原状态 |

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

- 拥有/可变 `dyn`、擦除拥有存储、Unit dyn Source、一般关联投影、长期借用与资源宿主 lending、一般异步取消/跨线程关闭和通用 Mutex/Transfer/Share 仍在后续责任单。
- GPU 首帧、字体、OS IME、完整 ZUI 与原生 Windows 执行不包含在本有限语言预览。目标/后端只按本次收据中真实执行的组合列出；Win64 交叉构建或 Wine 不等于原生 Windows 验证。
- `validation/product-gates.json` 中零 safe UB、同语义 Rust 运行/内存与编译/维护成本条件、至少一项可重复净突破及发布前提原样有效。目前不得宣称这些产品条件达成。B0、完整语言与 ZUI 终态验收继续独立进行。

本预览依据 `_zil_design.md` §20、`validation/product-gates.json`、`goal:workflow.md` 与实时 `goal:goal.md`。源码、各门结果及恢复范围绑定共同收据；远端恢复复用已准备的固定工具链和 Cargo 缓存，不代表空机器安装或旧历史重新验收。
