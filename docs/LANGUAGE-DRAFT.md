# ZIL 有限语言初稿预览（待共同验收，发布草案）

状态：**候选文案，未发布**。发布前由集成者填入同一精确源码提交、工具链、目标、执行收据和最终支持矩阵；本文件本身不证明测试通过。建议标签为 `ZIL language draft / experimental developer preview`，日期按 Asia/Shanghai，发布范围在验收后冻结。它不是 B0 Done、完整 ZIL-LANG-1.4、ZUI 或 Rust 产品目标达成声明。

## 支持矩阵

| 能力簇 | 预览拟支持范围 | 发布前证据 | 边界 |
| --- | --- | --- | --- |
| 普通集合与文本 | 真实 `std.store` 的有限 POD Record 集合、容量管理、push/pop、共享与独占元素窗口；现有 UTF-8 字面量/只读入口和显式 String | G9.3.2、G9.3.1 已接受收据；G9.3.3 待共同验收的源码、raw/Wire、两核、Clang/LLVMAPI O0/O2、分配失败与近邻拒绝 | 可变窗口须保持整 Store 排他，未知不同下标不能推定不交；一般拥有/异构容器、完整 Unicode/格式化另列 |
| trait 与借入迭代 | 现有普通泛型 trait、关联 Item、共享逐调用 lending；类型限定的静态关联函数及符号泛型宿主待本轮验收 | G2.4.1 已接受收据；G2.4.2 待具体/符号宿主源码、独立两核及机器结果 | 一般关联投影、资源宿主 lending、dyn 派发不随本片开放 |
| 普通程序组合 | 选定真实跨模块程序中的集合更新、错误传播、明确资源责任与短借用 | 固定源文件/输入/完整 stdout、退出状态、失败清理、合法及最近反例；精确提交与 O0/O2 结果待填 | 只晋升该组合实际覆盖的行为；其他 G3.4 长期借用形状按真实阻塞另行实现 |
| 交付与质量 | Source→Types→Analyze→独立 Verify→机器产物的有限 profile；CLI 区分 Success、SafeReject、Unsupported、ResourceLimit、InfraError | 源码和摘要绑定本批共同收据；完整默认/参考回归、两后端原生、失败清理及冷恢复分别记录，尚未执行不得记为通过 | 零命中、Unsupported、超时、工具失败与旧提交收据均非本次 Pass；正式 B0 和产品门维持原状态 |

## 快速开始（候选）

在具备 `rust-toolchain.toml` 指定 Rust、相应 LLVM/clang 与系统链接器的已支持环境中，从仓库根目录运行。LLVM 安装、环境变量和版本检查见 [LLVM 工具链](LLVM-API.md#安装与构建)；先 `cargo fetch --locked` 备妥依赖，再使用离线构建：

```sh
cargo build --offline --locked -p zil-cli
./target/debug/zil version
./target/debug/zil check examples/mainline/stream/main.zil --json
mkdir -p out
./target/debug/zil build examples/mainline/stream/main.zil -o out/stream --emit-dir out/stream-stages
printf '10 20 -3\n' | ./out/stream
```

该已存在的普通多模块样例预期输出为 `3 0 45`（含换行），退出 0；最终发布时须在所标源码与环境重新执行。本批组合样例在 `examples/mainline/draft_foundations/`，结合实际 std.store、trait 静态工厂、可变 helper 与显式失败处理。候选命令如下，只有同批机器验证通过后才记入最终收据：

```sh
./target/debug/zil run examples/mainline/draft_foundations/main.zil --entry main
# 完整返回值应为 42
```

## 未支持及环境限制

- `dyn`/擦除拥有存储、一般关联投影、长期借用与资源宿主 lending、一般异步取消/跨线程关闭和通用 Mutex/Transfer/Share 仍在后续责任单。
- GPU 首帧、字体、OS IME、完整 ZUI 与原生 Windows 执行不包含在本有限语言预览。目标/后端只按本次收据中真实执行的组合列出；Win64 交叉构建或 Wine 不等于原生 Windows 验证。
- `validation/product-gates.json` 中零 safe UB、同语义 Rust 运行/内存与编译/维护成本条件、至少一项可重复净突破及发布前提原样有效。目前不得宣称这些产品条件达成。`G10.6` 的 B0、完整语言与 ZUI 终态验收继续独立进行。

本草案依据 `_zil_design.md` §20、`validation/product-gates.json`、`docs/WORKFLOW.md` 与实时 `goal:goal.md`。发布时须记录精确源码和收据，不以文案或标签替代实现和测量。
