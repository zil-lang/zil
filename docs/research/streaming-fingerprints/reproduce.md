# 093 复算和复跑

完整 Git 历史保留 B `38b9169fde05fa0c0effc7fdd74296b9a6c20711`、
H `55521448f71e8381f72ea7c873fa930e084205de`、原 Rust 研究 harness
`130e9e34141796e0b16d4ff3b4c8dca1cac32cd9` 及最终计数修复。
最终生产文件已经恢复 B；研究源码仍能通过原提交独立构建候选。
工具链和源输入字节详见 `evidence/impl/093/h-identity.json`。

## 完整原始档案

归档路径、大小、SHA256 和逐文件核验见 `evidence/impl/093/raw-archive.json`。
归档内为 `raw/`，有独立 `archive-manifest.json`，逐文件记录路径、字节数、hash
与执行权限。它包含所有 093 运行及失败，不代表所有历届 ZIL 原始档案。

```sh
python3 tools/research/093/archive.py verify --archive /actual/path/research-093-raw.tar.gz
```

按正常 tar 工具解压后，`raw/frozen-003/` 为最终冻结实验。所有记录中的旧绝对
路径是实际来源，不应当作新机器的入口。不要在原冻结物上替换路径、覆盖样本或
复用旧输出目录；每次重跑创建一个新目录。

## 不重新计时的统计复算

只需要 Python 标准库。将实际解压的 `samples.json` 复制到一个新空目录，执行：

```sh
mkdir /actual/path/recompute-093
cp /actual/path/raw/frozen-003/samples.json /actual/path/recompute-093/samples.json
python3 tools/research/093/run.py analyze --out /actual/path/recompute-093
```

新 `summary.json` 应与归档中原摘要逐字相同。原执行已经这样核对，结果为
`evidence/impl/093/recompute.json`。全部 128 个进程都在输入中，固定排除的仅为
预登记 16 个预热进程；没有额外删样或离群处理。

## 原设计 Lab 与有限模型

归档 `raw/lab-input/zil-boundary-lab-0.6.0.zip` 是原始 ZIP，hash 与原分享输入相同。
先解压到一个新目录，再用当前工具对原实现进行只读身份检查和私有复制重放：

```sh
python3 tools/research/093/lab.py --archive /actual/path/zil-boundary-lab-0.6.0.zip --lab /actual/path/zil-boundary-lab-0.6.0 --out /actual/path/lab-replay-new
node --test --test-isolation=none --test-reporter=tap
python3 tools/research/093/structure.py --out /actual/path/structure-new/result.json
```

第二条命令的工作目录必须是新 `lab-replay-new/source`，它显示 147 个函数级测试；
Node 24 默认隔离模式的 14 个文件级测试不能误报为只有 14 个函数。该重放执行
原作者已有 U/K/D，不生成新的所谓同名求解器。有限积模型与 Lab 语义另行分级。

## 生产控制的重新构建

在带完整 Git 的最终源码树内，准备项目指定工具链，再执行当前版本控制器：

```sh
python3 tools/research/093/run.py prepare --repo . --candidate 55521448f71e8381f72ea7c873fa930e084205de --harness 130e9e34141796e0b16d4ff3b4c8dca1cac32cd9 --out /actual/path/frozen-new
```

它先提取精确 B/H 生产源码和 Rust harness，再由当前已修复计数注入器生成
独立计数版，串行 release 构建，记录二进制/源码/hash，并执行全部功能控制。
每个完成构建的可重建 target 会被带收据清理；源码、日志和实际 bin 保留。
`--harness` 提交里的旧 Python 注入脚本不是此命令的执行器；实际使用当前工作树
的 `instrument.py`，其 hash 单独记录。这个区分与原 frozen-003 的修复来源一致。

```sh
python3 tools/research/093/run.py verify --out /actual/path/frozen-new
/actual/path/frozen-new/bin/B-counted bridge /actual/path/bridge-new /actual/path/lab-replay-new/production-inputs
```

输入路径属于实际 SourceUnit 身份的一部分，所以换目录重跑的字节计数和摘要
可能变化；B/H 在同一路径内仍须逐字相同。本文不会把新环境输出重标为旧冻结结果。
原项目公开合同另见归档 `project-bridge-001/*-runner.command.json` 的实际参数。

`prepare` 不进行正式成本取样。若以后另开新实验，应先冻结相同功能控制、计划和
独占窗口，再执行 `sample`；本轮已经以 NoGo 结束，没有因回退追加样本或调参。
