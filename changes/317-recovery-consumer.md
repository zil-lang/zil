# 317 普通组件 API 恢复组合

以现存 299 主线 `f27a9330d` 重新施工，不继承无法恢复的 301–312
源码或机器成绩。只增加 `examples/mainline/recovery317` 和已有
`generic_regressions` 测试目标中的消费者，不修改编译器、schema、预算或产品门。

组件以普通库 API 连接三个前置：313 的有限 Copy 元组普通绑定、314 的
Type/Syntax 与纯 Copy 泛型、315 的普通局部完整 Product 可变 helper。
资源 Product 暂按 315 明确支持的 Mark 形状，不把 Store 资格当作已开放。
运行时使用已有 `c_with_callback` 登记—同步运行—关闭窗口，回调拥有资源，
调用者在窗口关闭后再释放回调环境。成功、业务错误和登记失败分别核对清理。
近邻拒绝覆盖普通 Product 活跃共享窗口与可变 helper 冲突、外部窗口中的
回调别名以及错误实例身份；非法输入不运行。

按固定依赖提交在本分树组合后，限测现有目标；实际命令、机器 stdout/退出码
与未完成项在交付时补记。仅声明当前源组合与 O0/O2 行为，完整语言、ZUI、B0、
原生 Windows 及 Rust 产品四目标不因本单晋升。

启动 `node tools/handoff.mjs --check` 实际 Fail：历史记录及实施工单摘要过期，
工具把当前分树识别为 main。施工权威按父派工指定 `/workspace/zil` 主树，
不修改交接元数据；Fail 不作为语言正确性成绩。

基线限定命令（`source /workspace/.zil-env/env.sh` 后显式
`CARGO_TARGET_DIR=/workspace/.zil-env/target317 CARGO_BUILD_JOBS=1`）：
`cargo test -p zil-driver --test generic_regressions recovery317_component_source -- --exact recovery317_component_source_and_nearest_rejections --nocapture`。
实际 0 passed / 1 failed / 14 filtered；消费者在 CT 阶段 `E-CT-SOURCE`：
`CT function must be a concrete ordinary function instance`，尚未到 Verify/native。
这保留 314 前置的实际缺口，不改成常量绕过。

318 对检查点 `ab2c4c4d7` 的只读初审指出：原元组累计 `2+3` 对调不敏感，
CT 只有 i32 一个具体实参，反例只断 SafeReject 可能被前置误拒遮蔽。修复保持
原检查点，补独立 first=2 / second=3 断言；同一 value_type 的 i32/i64、同一
Config 的两类实际参数和 phantom 参数身份；窗口结束后恢复的两个合法控制。
具体反例阶段/诊断待固定依赖组合的真实观察后绑定，不预填接受性结论。

同基线已有目标限定重放：`cargo test -p zil-driver --test generic_regressions -- --skip recovery317_`
实际 13 passed / 0 failed / 2 filtered，未跑全仓。

## 固定组合与实际结果

源码交付 `ef4505598`。在本分树按 root 明确授权依次 merge
`3cfc1e5a4f69e1ed95713d786a563830aff9782b` 与 `52b5ad0c3`，组合头为
`7be840f7d`，包含 313、315 和 314/320 的当时固定候选；没有自行合入主树。
后续 321 泛型 tuple 与 314/320 最终裁决不在本次运行身份内。

首次组合运行未显式启用 feature：317 两项中 native 通过、反例项失败，实际
只有 Clang O0/O2 × 三输入 = 6 次机器运行。作者曾误报为两后端 12 次，
检查实际 `compile.json` 后立即更正；不把旧结果重标为 LLVM API，也不累加重放。
反例失败的真实原因是 `Address<Closure>` 进入 move body 捕获先触发
Types / Unsupported / `E-UNSUPPORTED`。原字节保存在
`observed/unsupported_callback_address_capture.zil`，不宣称该源覆盖别名冲突。
最近反例改为调用者 alias 在 scope 返回后仍使用，配末用前移的合法控制。

最终在 `ef4505598` 执行（Rust 1.99.1、Clang 23.1.4，target317 / jobs1）：

```sh
source /workspace/.zil-env/env.sh
export CARGO_TARGET_DIR=/workspace/.zil-env/target317
export CARGO_BUILD_JOBS=1
ZIL_TEST_REPORT_DIR=/tmp/recovery317-report cargo test -p zil-driver --features llvm-api --test generic_regressions -- --nocapture
cargo clippy -p zil-driver --features llvm-api --test generic_regressions -- -D warnings
```

已有目标最终 **15 passed / 0 failed / 0 ignored / 0 filtered**，含两项317测试及
原13项；不是整仓测试。Clippy 首次因新增 `get(...).is_none()` 一项 lint 失败，
改为 `contains_key` 后限定目标通过。C宿主另以
`clang -std=c11 -Wall -Wextra -Werror -fsyntax-only` 通过。

317 正例及两个窗口末用控制各到实际 Verify。最终四份编译报告的真实 backend
依次为 Clang O0、Clang O2、LLVM API O0、LLVM API O2，各 verification_runs=1、
fallback=null；API 报告均为 builder 输入、verify_before_after=true、parse_ns=0。
每份各跑三输入，共 **12 次**，stdin `0/1/2` 的退出码分别 `0/1/2`，stdout分别
`REVSPBCAH` / `REVFPBCAH` / `RNBAH`，stderr均空。测试明确断 actual backend，
不是仅凭 feature 推断。

四个最近拒绝锁定实际阶段、code、根源文件及诊断覆盖的表达式，均不生成可执行文件：

| 源 | 阶段 | code | Verify 次数 |
| --- | --- | --- | ---: |
| shared_window | Verify | E-BORROW-CONFLICT | 1 |
| callback_alias | Analyze | E-BORROW-CONFLICT | 0 |
| nominal_config | CT | E-CT-CATEGORY | 0 |
| phantom_config | CT | E-CT-CATEGORY | 0 |

共享窗口由独立 Verify 拒绝；Analyze 发布的是未核候选，这与315确认的既有
架构一致。CT 两项诊断落在错误实际构造器上，正确 Config 的 main 同批到 Verify，
不以缺前置的任意 SafeReject 代替名义身份检查。

## 阶段与机器表示

最终保留产物 `/tmp/recovery317-report/recovery317-55677-{0,1,2,3}`，不入库堆积。
source input fingerprint 为 `4f8cab7823926b64ca2bc5d7beba3956fa17f33f74860f1058d34482cd7c65a7`；
API O0 `verified.txt` SHA-256 为 `b9de8fb28ba7083ca76b202537d4ec93f8e2e4f2ad884710ea98e9b7883d87e9`。
LLVM API O0 的实际 VerifiedIR 包含 `BorrowLocalMut`、`AddressField`、
`ExtractField` 和 `WithForeignCallback`；已核接口保留普通 `component.apply/event/open`
与具体 `bridge.run` 实例。实际 LLVM 中事件表示为 `{ i32, { i32, i1 } }` 并按
两层 `extractvalue` 投影；scope结果为 `{ i1, i32 }`。拥有环境实际内联表示为
`%zil_env_0 = type { { i32 }, i1 }` 和
`%zil_env_1 = type { { i32 }, i32, i32, i1 }`，没有把 CT Type/Syntax 做运行时值。
这些是实际生成产物摘录，不是手填 IR。

## Rust 四目标与待验

| 目标 | 本轮实证与边界 |
| --- | --- |
| 同类安全 | 已核有限窗口、实例身份、scope关闭及反例；非作者审核及完整语言安全未完成。 |
| 运行/内存 | 两后端O0/O2清理顺序与一次析构有证；未测Rust性能/RSS，本组件不声称分配失败覆盖。 |
| 表达/维护 | 普通库将泛型CT、tuple、可变Product helper和已有scope自然组合；完整框架维护收益未测。 |
| 编译成本 | 原预算及配置未变；本单无Rust成本比较或编译提速结论。 |
| 实质突破 | 原CT入口失败的真实多模块源进入Verify及原生；产品四目标合取未建立。 |

独审318已指出并推动源级oracle修正，最终审核与root后续集成仍待裁决。未跑全仓、
原生Windows、长期返回登记、异步/并发或性能门；完整语言/ZUI、B0及产品门保持。
