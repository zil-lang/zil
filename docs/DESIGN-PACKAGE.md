# ZIL 收尾设计与施工包

规范版本：ZIL-LANG-1.3。本文说明规范和最初派工输入的组成。当前仓库已有可运行的Rust标量编译器，实施事实以README、IMPLEMENTATION-STATUS和实际验收收据为准；这份设计说明本身不证明完整语言、内存安全或性能目标已经完成。

## 先读哪些文件

| 文件 | 用途 |
|---|---|
| `_zil_design.md` | 唯一完整语言与编译器规范；保留描述、执行资格和责任三者分离 |
| `_zil_goal.md` | 阶段、依赖与交付覆盖 |
| `docs/PARALLEL-PLAN.md` | 并行投放、写集、集成边界 |
| `docs/CODEX-START.md` | 可直接给Codex的当前接续指令 |
| `.task/B0.md`、`.task/T01.md`… | 14 张可分派工单，含具体接口、写集、测试与完成条件 |
| `docs/LANGUAGE-QUICKSTART.md` | 最后语法与闭包的紧凑例子 |
| `changes/REVIEW.md` | 从最后交付包到当前定案的理由与边界 |
| `lang-designer.html` | 离线阅读、目标比较和完整规范；由源文件生成 |

## 本包冻结的主要选择

- 自动推导闭包捕获；`move` 是按值构造环境，`give` 只转交实际 Managed 载体。Desc 复制不接管责任，Address 不因 move 延寿。
- `impl Type(self: ...)` 声明组默认接收者；`.name` 绑定词法 self，trait impl 可依接口补 self。
- `fn(A)->R` 是无捕获函数指针；匿名环境用泛型 Fn/FnMut/FnOnce、受限 `-> impl Trait` 返回或显式 dyn 存储。
- 记录构造支持期望类型省略，spread 按已定 Copy/Move 与清理顺序执行；管道仅一元调用，元组展开须显式。
- Native 使用显式 main 与统一安全规则；GC/托管宿主不成为隐含施工依赖。

## 工程输入

`spec/grammar.ebnf`、`spec/attributes.json`、`spec/interfaces.json`、`spec/diagnostics.json`、`policy/dependencies.json` 与 `validation/product-gates.json` 是具名规范附件。`data/normative-inputs.json` 同时锁定这些输入；只对正文取 hash 不够。

`conformance/` 包含 93 条独立源码验收输入及预期，全部为 NotRun；其中 6 条依赖明确标为 NotImplemented 的资源夹具。`validation/task-test-plans.json` 另列 63 项需在施工时补齐的集成、协议、故障注入和产品测试要求。它们不是 156 项已通过测试。

最初设计输入中所有工单为Planned；当前B0已实施但公共schema交接未完成，其他正式状态以主树.task为准。接续现有实现应读CODEX-START，不要按初始计划重新创建编译器。B0正式完成后仍按依赖投放。未访问、未修改用户现有 Windows/Zig 仓，不能把历史进度当成新编译器完成度。

## 检查本包

使用 Python 3.11+ 和 Node.js，可从包目录运行：

```sh
python3 tools/update-indexes.py
python3 tools/update_designer.py
python3 tools/check-docs.py
python3 tools/check-plan.py --self-test
python3 tools/check-conformance.py
node --test tests/architecture.test.mjs tests/hash-syntax.test.mjs
python3 tools/package_check.py
```

前三个生成/检查步骤不要在多个 worktree 同时回写同一主树。修改源后由集成者重建索引和 SHA256SUMS；原始包仅核对时不需要重新生成。`tools/check-browser.py` 是需要 Playwright Python 与浏览器的可选页面回归；本次实际执行与未执行范围以 `evidence/REPORT.md` 为准。

工单中的 `tools/run-cases.py` 和正式 Rust 编译链是 B0 后的施工产物；当前包没有用假的 runner 返回 Pass。

`data/original-atlas.json` 等历史比较资料保留作输入快照，其余语言资料本轮没有重新逐项核实；当前 ZIL 数据与正文已同步。旧包截图、旧测试成绩与下载缓存没有充当本次证据。
