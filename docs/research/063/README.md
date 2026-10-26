# 063：编译成本与联合关系表示

这是draft12/schema9上的只读影子研究，入口`tools/research/063`。复用059的有限语义和直接raw枚举oracle，新增阶段/事件/位方程故障定位、完整持久根可达节点诊断、融合关系积、内部变量顺序对照。059历史与生产安全核均保持原字节。

## 实验问题

观察到四个独立条件触及300k节点，不能据此认定真实关系本身必须这么大。先分别检查累计分配量、完整持久根可达量、单事件表达式临时量；再区分表示、算法、调度与输出材料化成本。

`mono`连接全部位方程再消元；`cut`最后关联提早消元；`wire`再用唯一新鲜输出的恒等/取反代换；`fused`使用与cut相同因子顺序和消元时机，只将AND和Exists融合成标准relational product。`:owner`和`:token`后缀只改变BDD内部rank，规范坐标、因子顺序、oracle编码和全量输出不变。

`ZIL_063_DIAGNOSTICS=1`启用有界的持久根普查。该工作单独计量；含诊断运行只用于故障解释，不用于正式速度裁决。默认计时禁用普查。完整根集合包括所有已完成事件的方程、fault、abort、CFG/出口guard、固定点states和已记录relations；计划引用相同事件方程。快照发生在没有未发布临时的安全边界。失败时保存精确阶段/point/bit和分配量，最后快照不是失败瞬间的全部活跃节点，不能冒充它。

支持域继承059：有限Bool相关、0..15/Top整数抽象、单活动site incarnation、完整token/carrier/retirement/Desc/order关系。新schema9 package/操作明确Unsupported。完整函数的ResourceLimit、oracle限额或Unsupported均非Pass。

## 对照原则

1. 同一真实源、TypedIR抽象、关系出口与完整canonical字节；不以边缘分布或摘要hash替代关系等价。
2. 性能必须计入抽取、事件构造、计划、求解、核对、普查/材料化、序列化和析构。研究oracle、前端与全进程另列，完整JSON封装/进程启动亦留wall证据。
3. Rust样本阶段诊断只说明该源码、编译器、配置；cargo check、debug native、optimized native和增量编辑是不同任务。
4. 理论与2026最新实现见[SOURCES.md](SOURCES.md)，总体架构判断见[ARCHITECTURE.md](ARCHITECTURE.md)，固定实验结论见[RESULTS.md](RESULTS.md)。没有净成本和生产语义闭合证据，仍不入生产核。


## 回收与复现

方法第三段`:gc`启用安全边界回收，如`mono:canonical:gc`或`fused:owner:gc`。仍最多占300,000物理arena槽位，但累计创建量可以更大；标记、按bucket容量扫描hash表、清空缓存、按存活NodeId重建unique表与free列表均计入同一候选Work。原未回收算法和纯排序失败会另外保留，不能将恢复成功归因给cut独有机制。

```sh
python3 tools/research/063/run.py prepare --repo . --candidate <精确研究提交> --out out/runs/research/063/<新freeze目录>
# prepare完成且root协调其他构建/压力/计时停止后：
python3 tools/research/063/run.py sample --out out/runs/research/063/<新freeze目录> --window <root窗口身份>
python3 tools/research/063/run.py verify --out out/runs/research/063/<同一freeze目录>
```

先加载项目工具链环境；Rust离线、release、debug=0、incremental=0、jobs=1。prepare从独立70d生产archive覆盖指定063提交，测试、clippy、构建、完整关系和actual-driver emit/native核对；lazy-source只能与baseline相差唯一补丁的三个文件。sample不构建编译器，拒绝已有样本，禁用诊断普查。所有源码/二进制/输入/冻结产物均有SHA256；单个方法未完成仍完整保存，但不进入成功时长倍率。

测量包含2轮预热、7轮保留样本、轮换执行顺序、全过程JSON/命令。Rust的4次`-Ztime-passes`属于独立小样本诊断，显式使用`RUSTC_BOOTSTRAP=1`；不与ZIL报产品倍率，也不把重叠阶段相加。开发期首次失败和修复另保留，不覆盖原日志。

下一表示的集成后静态复核见[SPARSE-CONTRACT](SPARSE-CONTRACT.md)：保留原操作数出现次数、精确fault关系与逐步解码对应，不把唯一位置先验当安全证明。
