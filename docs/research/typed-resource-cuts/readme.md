# 055：从真实 TypedIR 检验倒序图切割

当前schema8主线已经合入[独立功能适配](SCHEMA8-INTEGRATION.md)：真实四模块的19函数全部通过shadow。历史3192个性能样本仍固定下述schema7身份；重放正式冻结须先从历史bundle检出`3d4375a18ea4ab9c4e800461befea1d7f3c69f42`，不能在当前schema8源码上跳过哈希检查。当前源码的功能测试命令为`cargo test --manifest-path tools/research/055/Cargo.toml --offline --locked`。

055把050的研究推进到真实ZIL源文件，经生产前端得到TypedIR，并选择资源核里的Desc/借入闭包may-liveness作为明确可证伪的子问题。它保留完整原点关系和同一有标签资源/控制事件图；一般联合资源状态求解仍未实现。生产crate、接受集合及可信边界均不变。

## 阅读顺序

- [RESULTS.md](RESULTS.md)：冻结身份、全部形状的实际成本、退化与入核结论。
- [CONTRACT.md](CONTRACT.md)：三层保证的区别、抽象失败出口、限制和预先固定的计时要求。
- [PROOF.md](PROOF.md)：恒等点消去/逆序恢复和观察事件关系的限定等价律。
- [SOURCES.md](SOURCES.md)：原始切割启发、已有消元框架与Polonius的主源及迁移边界。
- [SCHEMA8.md](SCHEMA8.md)：真实错误清理项目的独立功能补充与最小适配；不混入schema7计时。

实现位于`tools/research/055`独立Rust workspace；19份真实源码清单为`tests/perf/impl055/cases.tsv`。7份来自已有资源闭包样例，其余是确定生成的ZIL源码；人工代数填边压力例只用于测试，不进入性能语料。

## 重做

在对应Git身份的仓库根使用Rust1.99.1环境。本次主试验生产依赖冻结于draft10提交`7e4c4728c09fd07a55680844514e0f38bb29e708`、TypedIR schema7。后续schema8功能补充不能复用这份性能身份。

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1
python3 tools/research/055/run.py freeze \
  --folder out/runs/research/055/new-unique-run
```

freeze要求研究实现、合同及生产依赖已提交且未修改；运行限定测试/clippy、release构建、生产验证、raw抽取审计、独立oracle和三种影子算法，保存完整TypedIR/预期关系、源哈希、工具链/profile、二进制身份及压缩输入包。源码不变且其它构建停止后，在集成方安排的独占窗口执行：

```sh
python3 tools/research/055/run.py bench \
  --folder out/runs/research/055/new-unique-run \
  --idle-window coordinated-session-name
```

每法预热一次，6轮×7次，flat/cut/block/production交错；固定一个允许的CPU，完整保留所有样本。生产全Analyze+Verify只作成本背景，语义范围大于影子查询。独立oracle参照在冻结时产生，样本包含raw审计和完整参照核对，不能把它当未来生产中的免费动态证明。

`INVALIDATED.json`标识独审后失去正式资格的冻结，不可进入计时。第一次bc7c24e冻结因预算合同和漏计问题失格，仍保留功能收据，正式数据采用修复后的独立身份。各阶段100M计费上限及N−1/N/N+1反例见合同；它们不是进程CPU/RSS的总额保证。
