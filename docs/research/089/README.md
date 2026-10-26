# 089 成功语法资格与发现阶段复用

本目录跟随主树工单 `.task/impl/089.md`。研究冻结schema15，与语言主线schema16施工分开。作者结论为实验完成、K/L/KL生产NoGo，待非作者复核；当前生产crate树已逐字恢复B。独立核、产品门和完整语言状态不由本研究改变。

## 四个精确候选

| 控制 | 生产源码提交 | 差别 |
|---|---|---|
| B | `2883f978de79ea0e8a09a4cb218fa68687f4f568` | draft18生产强基线 |
| K | `9b501ae9d6e6e1892695508c9c4cc110d09a6a77` | 仅成功Parse的token/node预算资格分离 |
| L | `62bbb97dcaa3879776875d7ac1fa2e8b6e834080` | 仅原精确键的发现syntax复用 |
| KL | `351e6de6a2643f7e1bacd4554d3f0f258a3ad5a1` | 两机制组合 |

K提交的父历史含L控制，但该提交的实际生产树只含K；执行按精确树字节，不按提交标题或祖先猜测机制。准备器提取每棵Git树并记录全文件清单。

功能/计数/正式性能harness冻结在 `2bd4ed9f3bc7c3f9fdbb665bf047b803f19a6c7b`；候选身份不包括随后仅增加的研究文档。最终生产NoGo，当前生产文件恢复基线，四个冻结研究对象仍保留在完整Git历史。仅实验候选适用的两份测试移至 `tools/research/089/frozen-tests`；原可执行版本仍在K/KL精确提交中。

## 内容

- [结果与裁决](RESULTS.md)、[全部配对表](TABLES.md)、[下一可证伪实验](NEXT-EXPERIMENTS.md)。
- [预登记](PREREGISTRATION.md)：事前固定样本和控制、停止与披露条件。
- [架构边界](ARCHITECTURE.md)：为什么预算资格可拆、纵向共享省了什么、私有收据不授予Verify权限。
- `tools/research/089/run.py`：冻结八个二进制（四计时、四直接入口计数），先完整功能比较，再在root授权窗口采样。
- `tools/research/089/archive.py`：归档明确选择的完整raw根并逐member核对。

## 重建契约

使用配套完整历史和已记录Rust/clang工具链，在新的输出目录运行：

```sh
python3 tools/research/089/run.py prepare \
  --repo /absolute/path/to/zil-checkout \
  --harness 2bd4ed9f3bc7c3f9fdbb665bf047b803f19a6c7b \
  --out /absolute/path/to/new-089-replay
```

prepare只重建并跑功能/计数，不把它称为历史性能复测。正式sample命令需要明确独占窗口；不要因重新运行得到更好数字而替换旧样本。完整研究状态以最终作者、非作者和sample收据为准，不以本说明预先宣布Pass或生产Go。

## 有界冷恢复

交付恢复可运行以下较小功能闭环，已由作者在全新目录实际通过；它不产生历史性能的新样本：

```sh
python3 tools/research/089/replay.py \
  --repo /absolute/path/to/zil-checkout \
  --out /absolute/path/to/new-089-bounded-replay
```

重建冻结B/KL的四个debug0二进制（各普通/计数），执行332个compiler请求：284个guard、24个完整Build、24个计数Verify；24次原生行为。guard和产物按相同源字节比较，64/512模块及默认容量控制核实际lexer/parser次数。计时结论仍只来自冻结release采样，不把新debug执行改标为性能结果。精确统计复算契约见 `evidence/impl/089/statistics-contract.json`；作者run.py的analyze不是非作者独审。
