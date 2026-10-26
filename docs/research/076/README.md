# 076：强显式基线与完整行共享

此研究只在冻结schema10生产与071算法之上运行。076代码不进入生产crate工作区。
派工时的规范身份为schema12，当前整轮交付已推进至schema13，研究执行依赖仍固定schema10。
三者分别保存；原预注册的schema12身份和旧测量不重新标识。

## 固定方法

071五方法直接复用冻结实现；补齐Dense/direct控制，并新增Dense/intern和Local/intern。
intern驻留完整物理位向量，确定性哈希完整判等，关系成员和队列只保存行ID；计划预登记
关系键。所有RHS/free谓词使用同一个原始完整前态。最终展开回既有有序完整关系，公共
checker、独立oracle和canonical序列化保持合同。新增引擎指标列在每函数`intern_stats`。

成员上限仍统计所有关系的成员总数。额外的驻留唯一行和预登记关系也受限；候选可因
这些准备成本更早ResourceLimit，失败原样报告，不当成语义成功。
哈希碰撞显式逐字核对，扩容重建索引，原行ID不变。没有抽样集合、概率等价或状态剪枝。

## 复现

先加载交付工具链，再以精确候选提交构建独立冻结树：

```sh
python tools/research/076/run.py prepare --repo . --candidate COMMIT --out out/runs/research/076/frozen-001
python tools/research/076/run.py verify --out out/runs/research/076/frozen-001
```

prepare包括release限定测试、四条跨新进程预算复验命令、clippy、构建、16族八方法完整
函数结果与独立canonical逐字核对。四条命令覆盖五个精确N：sparse、Local direct、
Dense direct各一个，intern命令同时复验Dense和Local两个N；每项均检查N成功、
N−1 ResourceLimit。命令/源码/二进制/输入/输出完整哈希可审计。
prepare内记录的计时只说明命令执行，不能升格性能证据。采样必须由root协调独占窗口：

```sh
python tools/research/076/run.py sample --out out/runs/research/076/frozen-001 --window ROOT_WINDOW_ID
python tools/research/076/analyze.py --out out/runs/research/076/frozen-001 --receipt evidence/impl/076/results.json
python tools/research/076/archive.py --root out/runs/research/076 --receipt evidence/impl/076/portable-raw.json
```

`source`/`target`可以从提交和清单重建；portable archive保留实际命令、全部canonical输出、
失败尝试、正式采样和派生统计，不能用只有汇总数字的报告替代。

## 解释边界

四项Rust对标目标的本轮状态：安全仅覆盖冻结有限关系逐字等价，不等于全语言证明；
运行时/内存和源代码/维护无本轮新产品测量；编译效率仅测完整研究查询，不等于总编译。
突破候选是重构表示/关系执行的可测归因；行驻留属于已有显式状态技术，不能称新定理，
不能由资源唯一位置推出全部安全，也没有解除最坏情况的状态爆炸。
结论以新增强基线为准，负结果同样记入RESULTS。
