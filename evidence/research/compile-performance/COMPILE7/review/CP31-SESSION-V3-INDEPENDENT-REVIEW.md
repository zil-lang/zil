# CP31 持久会话工具 v3 独立审核

结论：**Pass，限冻结工具与证据门；不代表候选编译器或性能通过**。实现作者 incremental_research，集成 root，审核 cp25_evidence。根提交 `cfb54cf21` 的三份文件与交付摘要一致，完整提交号与逐文件 SHA256 见配套 JSON。Rust harness 与 v2 相同，v3 只修 Python 证据处理。

## 计量范围

每个独立进程建立一个持久 CompilationSession，按固定七请求运行 cold、unchanged、private、public、restore、private_again、restore_again。compile_ns 真正包住 compile_path 到 Verify，包含源发现与本请求编译工作。编辑快照复制、session 初始化、报告生成时间另列；wait4 再保留整个进程 wall/CPU/最大单进程 RSS。它不包含 LLVM 生成与链接，不能称完整 AOT build 提速。七请求序列也不是旧四请求加权 CLI 窗口。

control 模式对每个请求再开 fresh session，比较 status、phase、diagnostics、input fingerprint、source_files、scalar_interfaces，并要求 fresh 与 persistent 两者都成功且当前 Verify 执行一次。测量不执行额外 fresh 编译，以免混入计时。该对照只证明公开报告观察量，未输出完整 TypedIR 或原生程序 oracle。

freeze/validate 绑定 runner、harness 源、实际 A/B 二进制、固定 fixture 文件集、显式工具、编译环境摘要、CPU 型号/配额/affinity；窗口前后均检查。源码提交字符串是构建归属声明，仍需要根另附精确编译器/工具链/构建收据，不能由该字符串独自证明二进制来源。

## v2 退回与 v3 修复

原 v2 的三个问题与 NeedsChanges 收据保留，不追改成通过。

1. v3 对整数 ns/计数、有限 CPU、计分量正值、初始化/准备量非负值严格校验，拒绝 bool、字符串、负数、NaN/Inf及非整数 ns；同时核对七请求与七 snapshot 的对应顺序。
2. 成功 control 返回每个 case/capacity/request 的 observable map，测量结果必须与其完全一致。因此两臂共同漂移不能仅靠彼此相等通过。
3. 窗口结束重新读取 rows.jsonl 原字节，解析结果须与执行记录相等，摘要与计分绑定同一份数据。原始 stdout/stderr、control 摘要与计划也再次核验。

任何上述失败保留原始行、日志与失败 result，不生成 summaries。工具继续使用独立新输出目录，预检不计分，正式计量需显式 quiet-window；此参数只是声明已停止已知构建/测试，不保证宿主机独占。

## 非作者实际控制

审核已读完整 Python/Rust harness、v2→v3差异和控制脚本，然后使用根提交中的 runner，向全新审核目录执行 `compile7_session_v3_checks.py`。**67 项通过，0 失败，退出 0**；未启动编译器子进程、未运行性能窗口。

其中复核已存在的 baseline v3 真控制：4 个持久会话序列，共 28 个 persistent 请求及其 28 次 fresh 对照，原始日志/计划/工具身份保持不变。负控覆盖原始/重摘要日志、假 Pass、顺序/argv、工具内容/realpath、环境、无 quiet 门禁，另含 35 个数值反例、合法零准备成本、七个 snapshot 错配。两个合成完整 run-finalization 负控分别使两臂共同 observable 漂移、改写磁盘行；结果确为失败且不含 summaries。

完整命令、脚本与源摘要、控制 result 摘要见 `CP31-SESSION-V3-INDEPENDENT-REVIEW.json`，实际输出 `CP31-SESSION-V3-INDEPENDENT-CONTROLS.log`。67 是非作者复跑控制数，不与作者同一套 67 项相加，也不把既有 baseline 控制当作 CP29/CP30 候选验证。候选真实控制、完整计量、结果复算与采用仍由根另行完成。
