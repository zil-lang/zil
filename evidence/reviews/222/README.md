# 222 非作者独立审核

审核者 `/root/product_store`，实施者 `/root/ct_for`。最终有限切片裁决为 **Pass**，绑定实施代码 `566bda6a12ab9dbe9be5adbf5b870a411189dc37`。实际独审 checkout 是 `4ddd6a6354619366ac47c42311e925ee646c7898`，与实施提交仅相差原独立测试文件；生产源码无差异。精确命令、CLI 与测试可执行文件 SHA、规范输入及原始运行见 `RESULT.json`、`PROVENANCE.json` 和 `raw-evidence.tar.gz`。

## 两次实际退回及修复

第一轮在 `a54b7cd5` 冻结五份原始源码，分别把迭代标量用在函数参数、局部注解、返回类型、闭包参数和泛型类型实参的数组长度中。普通手写长度对照通过，五份生成源码却没有完成既有标量表达式槽的替换。最终同一批源字节全部到达 Verify。

第二轮使用全新独立 target，在 `7265b8db` 的 `compile_source` API 上证实：普通 `Cell { x: 3 }` 在同名值局部存在时可以通过，等价的单次生成源码却因名称卫生改写而被 Types 拒绝。原测试提交为 `ed77523f9`，原失败为 1 Pass / 1 Fail。`566bda6a` 保留构造器的名义名称位置，同时继续遍历泛型类型/常量参数、普通值、base 和字段表达式；原始两源现在均通过。

原有 CLI/project 入口对手写同名构造器对照也会拒绝。这与独立单源 API 的表现分开记录；初始三条未经控制证实的 CLI Success 预期不计作三个编译器缺陷。最早一个负夹具误用了保留字 `use`，其 Parse 拒绝也不计作 CT 边界通过；另行冻结的 `take` 版本才证实 Ct 拒绝。所有原始文件和观察均保留。

## 最终独立执行

| 项目 | 实际结果 |
|---|---|
| 原独立测试 | 2 selected / 2 passed / 0 failed / 0 ignored |
| 原 TypeRef 反例 | 5 份逐字重放，全部到达 Verify |
| 累计预算 | Core CT steps 77；全链 CT steps 96；全链 nodes 1084；与前次相同 |
| 预算边界 | fresh 最小值、N、warm N、N−1 ResourceLimit、再次 warm N；active query 为零，成功请求只有一次 Verify |
| 实际机器程序 | 1 个独立资源程序，LLVM API builder，Linux x86_64，O0/O2 各 4 次，共 8 次 |
| 模式 0 | 普通提前返回，exit 0，析构输出 `BAZ` |
| 模式 1 | 运行期 continue，exit 0，析构输出 `CDCBAZ` |
| 模式 2 | 运行期 break，exit 0，析构输出 `DCBAZ` |
| 模式 3 | Unit 错误返回，exit 1，析构输出 `CBAZ` |

资源程序的源、oracle 和独立 runner 在 `consumer/`；oracle 在执行前冻结。生成出的两个外层资源保留到接收作用域退出，实际析构顺序证明生成花括号没有额外制造运行期资源作用域。该程序也实际使用 ByteBuffer，但没有做分配故障注入或 live allocation 计数，不能据此声称 allocator 故障全部闭合或 live=0。

作者把独立原测试晋升为 `impl222_review_regressions.rs` 后的执行仍属于作者复跑；不与上述独立计数相加。原 30 条 CLI 观察也不会重新贴成最终提交执行过的 30 条结果。

## 共同候选及有限边界

`../222-224/REVIEW-222.json` 单独绑定共同候选 `4d7bbb762eb34987ffd05c9ab23a6ad6bc6fae41`。CT materializer、hygiene、TypeRef 遍历及 Expand CT selection 的 8 个生产文件逐字相同。其他 18 个生产文件变化属于 Product Store 和共享 Product 地址路径，精确名单已记录。本审核只批准共同候选中的 222 范围；审核者是 223 作者，不在这里自审 223。没有把 566b 的实际执行改记为共同候选的执行，全量组合验收由集成方处理。

本片只开放普通函数及同型标量数组/range 的有限源级生成。一般 Type/Syntax 生成、类型声明生成及完整语言均没有完成。普通 Core lowering 对 `defer` 仍走后续能力的 Unsupported 分支，独审只检查了生成遍历，不报告 defer 的机器执行通过。Windows、正式全仓、Rust 运行/内存或编译速度比较、产品门均未在本独审运行。

早期共享 `targets/223` 的历史构建单独注明；后续另一 checkout 暴露 Cargo 缓存污染后，该目录及相关缓存已由集成方作废。最终所有接受与机器证据均来自专用 `targets/review222`。每份 receipt 保留实际 manifest、cwd、Git SHA 和 CLI hash。

## 重放入口

原测试在 `crates/zil-driver/tests/review222_independent.rs`。执行时设置独立 `CARGO_TARGET_DIR` 与 `ZIL_TEST_REPORT_DIR`，使用仓库对应 LLVM API 环境，再运行：

```sh
cargo test --manifest-path /path/to/review-tree/Cargo.toml --locked \
  -p zil-driver --features llvm-api --test review222_independent -- --nocapture
```

归档中的 `review222/replay_repair.py` 保存本次绝对路径及精确五源重放命令；机器 runner 的 `--cli`、`--build-receipt`、`--out` 参数分别绑定实际编译器、构建收据和新输出目录。使用其他 checkout 时应建立新的路径/提交收据，不重用旧缓存身份。

有限源生成和两项独立缺陷闭合是本次语言增量。四项 Rust 产品目标的合取、同能力性能优势以及产品级净突破仍未建立。
