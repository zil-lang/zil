# ZIL 语言初稿开发预览

状态：**有限 profile 已验收，实验性开发预览**（2026-11-07，Asia/Shanghai）。实测源码 `a8c7953e7f4f5932f838284390e55f1f8884bdbc`，Rust 1.99.1、LLVM 23.1.4、Linux x86_64；完整结果见[共同验收](validation/G10.6.1-common-acceptance.json)。默认 5190/0、参考 5078/0、文本 70/0，双配置严格 Clippy、原生消费者、10 项工程检查及远端源码恢复构建通过。它不是 B0 Done、完整 ZIL-LANG-1.4、ZUI 或 Rust 产品目标达成声明。

本轮按普通多模块程序的闭环选必备项：已有类型、资源责任、错误传播、文本和借入迭代继续复用已交付实现；补齐阻止普通集合与泛型 API 组合的静态 trait 调用、元素原地修改和直接标量序列，再统一验收构建与恢复入口。更广动态派发、异步与平台支持维持明确缺口，不由预览文案推定完成。

本批有限扩展在 `498f0f5a1e7595e12e5303750ef7686b1a7dc41b` 的共同验收中记录：普通 POD/九种标量 Store 可作为有限 Package、Tuple、Enum、ErrorUnion 的完整拥有字段，并可进入自包含 Fn/FnMut/FnOnce 拥有捕获；共享借入 dyn 可在两个 Copy struct 提供者间按固定标量槽派发。完整门默认 5248/0、参考 5136/0、文本 70/0；Source/两核/两后端原生与独审、远端源码构建运行各自见[本批收据](validation/ordinary-ownership-and-shared-dispatch-acceptance.json)。Unit dyn Source 仍由 Types 返回 Unsupported；拥有或可变 dyn、擦除存储、一般 lending 不在本批。

## 支持矩阵

| 能力簇 | 预览支持范围 | 已验收证据 | 边界 |
| --- | --- | --- | --- |
| 有限拥有组合与共享动态派发 | 普通 POD/九种标量 Store 的有限拥有父形状和自包含捕获；两提供者的共享借入 dyn 固定标量槽 | 精确源码的 Source、独立核、Clang/LLVM API O0/O2、共同门及冷恢复见本批收据 | Unit dyn Source Unsupported；无 owning/mut dyn、擦除拥有存储或一般 lending |
| 普通集合与文本 | 真实 `std.store` 的有限 POD Record 和九种基础标量集合、容量管理、push/pop、共享与独占元素窗口；现有 UTF-8 字面量/只读入口和显式 String | G9.3.2、G9.3.1 已接受收据；G9.3.3/G9.3.4 同批验收的源码、raw/Wire、两核、Clang/LLVMAPI O0/O2、分配失败与近邻拒绝 | 可变窗口须保持整 Store 排他，未知不同下标不能推定不交；一般拥有/异构容器、完整 Unicode/格式化另列 |
| trait 与借入迭代 | 现有普通泛型 trait、关联 Item、共享逐调用 lending；类型限定的静态关联函数及符号泛型宿主已通过本轮验收 | G2.4.1 已接受收据；G2.4.2 具体/符号宿主源码、独立两核及机器结果 | 一般关联投影、资源宿主 lending 不随该历史片开放；本批仅有限共享借入 dyn |
| 普通程序组合 | 选定真实跨模块程序中的集合更新、错误传播、明确资源责任与短借用 | 固定源文件/输入/完整 stdout、退出状态、失败清理、合法及最近反例；精确提交与 O0/O2 结果见共同收据 | 只晋升该组合实际覆盖的行为；其他 G3.4 长期借用形状按真实阻塞另行实现 |
| 交付与质量 | Source→Types→Analyze→独立 Verify→机器产物的有限 profile；CLI 区分 Success、SafeReject、Unsupported、ResourceLimit、InfraError | 源码和摘要绑定本批共同收据；完整默认/参考回归、两后端原生、失败清理及冷恢复分别记录，全部通过，冷恢复边界单独记录 | 零命中、Unsupported、超时、工具失败与旧提交收据均非本次 Pass；正式 B0 和产品门维持原状态 |

## 快速开始

在具备 `rust-toolchain.toml` 指定 Rust、相应 LLVM/clang 与系统链接器的已支持环境中，从仓库根目录运行。LLVM 安装、环境变量和版本检查见 [LLVM 工具链](LLVM-API.md#安装与构建)；先 `cargo fetch --locked` 备妥依赖，再使用离线构建：

```sh
cargo build --offline --locked -p zil-cli
./target/debug/zil version
./target/debug/zil check examples/mainline/stream/main.zil --json
mkdir -p out
./target/debug/zil build examples/mainline/stream/main.zil -o out/stream --emit-dir out/stream-stages
printf '10 20 -3\n' | ./out/stream
```

该已存在的普通多模块样例预期输出为 `3 0 45`（含换行），退出 0；本批已在远端重新拉取的同一源码与所标环境执行。本批组合样例在 `examples/mainline/draft_foundations/`，结合实际 std.store、trait 静态工厂、可变 helper 与显式失败处理。开发运行命令如下（本批恢复实跑使用 release CLI）：

```sh
./target/debug/zil run examples/mainline/draft_foundations/main.zil --entry main
# 完整返回值应为 42
```

## 未支持及环境限制

- 拥有/可变 `dyn`、擦除拥有存储、Unit dyn Source、一般关联投影、长期借用与资源宿主 lending、一般异步取消/跨线程关闭和通用 Mutex/Transfer/Share 仍在后续责任单。
- GPU 首帧、字体、OS IME、完整 ZUI 与原生 Windows 执行不包含在本有限语言预览。目标/后端只按本次收据中真实执行的组合列出；Win64 交叉构建或 Wine 不等于原生 Windows 验证。
- `validation/product-gates.json` 中零 safe UB、同语义 Rust 运行/内存与编译/维护成本条件、至少一项可重复净突破及发布前提原样有效。目前不得宣称这些产品条件达成。`G10.6` 的 B0、完整语言与 ZUI 终态验收继续独立进行。

本预览依据 `_zil_design.md` §20、`validation/product-gates.json`、`docs/WORKFLOW.md` 与实时 `goal:goal.md`。源码、各门结果及恢复范围绑定共同收据；远端恢复复用已准备的固定工具链和 Cargo 缓存，不代表空机器安装或旧历史重新验收。
