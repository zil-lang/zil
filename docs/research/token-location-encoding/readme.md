# 067：守恒启发的精确token位置实验

## 交付范围

这是冻结在draft13生产源码`400a082444275e1d0e2ad78c4910b9b6fc526be5`、TypedIR schema10上的独立研究。它继承063的有限联合关系语义，实际实现一种位置编码，保持直接解释raw TypedIR的独立oracle；不修改生产分析器、核验器或语言接受性。后续生产schema迁移不自动进入本冻结实验。

入口为`tools/research/067/Cargo.toml`中的`cut067`。预算与063一致：每个函数候选和oracle各100M工作、30万物理BDD槽、60万缓存项、10万行、4096语义位、2万程序点。源码抽取仍受原始完整语义位预算约束；压缩不能绕过预先声明的输入边界。

```sh
cargo run --offline --locked --release --manifest-path tools/research/067/Cargo.toml -- \
  tests/perf/impl059/correlated.zil location:fused:owner:gc /tmp/zil067-relations
```

方法格式为`dense|location:mono|cut|wire|fused:canonical|owner|token:gc`。正式矩阵固定owner顺序下dense/location × mono/fused四种方法，完整结果目录包含候选与oracle的canonical JSON。标量值、描述引用、初始化顺序、退休记录、原始事件标签和所有逐点/出口/fault/abort/unsupported关系一并输出；任何差异使实验失败。

## 机制

设一个函数有C个carrier、T个责任token。原表示对每个`(carrier, token)`保留一位`i:c:t`，共CT位。正常态中同一token至多处于一个carrier，故候选为每个token保留位置`p:t∈{0,…,C}`，0表示当前不在任何carrier，约需`T ceil(log2(C+1))`位。C=0时采用一位编码并限定0。

仅关联矩阵被替换。所有状态、fresh、retirement、Desc、guard、标量与清理顺序坐标逐位保留。多个token可使用同一位置码，因此一个闭包可拥有多个捕获资源及闭包本身的责任token。不同程序路径仍在同一个完整联合关系里；绝不把各token分别计算后笛卡尔积。

这不是通用“唯一位置即安全”的判定：唯一位置不能证明清理恰好一次，不能证明正常出口没有遗留责任，也不能证明原始调用/捕获没有重复使用同一实参。上述观察仍由原模型决定。

## 等价条件与可执行保持检查

令D将有效位置码解回完整关联列，E在每列至多一个1时编码。此域上`D(E(s))=s`且E单射。对语义状态集R及原始转移F，候选需要保持完整观察，而非只保持一个“安全/不安全”结果。

1. **初态。** 从原完整初始坐标构造位置，逐列检查是否有多处为真；若有则失败，不能消重。其他未知输入坐标保留原来的非确定性。
2. **事件。** 原始操作按原顺序形成fault和abort谓词，再形成同时更新的dense语义表达式。所有观察谓词使用同一原始输入R；某个fault被发现不会删掉同时成立的其他fault。重复实参或重复闭包捕获的检查发生在任何转移消重之前。
3. **正常像。** 仅在原模型已定义的正常域G上求像。对每个被修改token，将各下一态owner谓词记为q1…qC。候选构造`overlap = OR(i<j)(qi AND qj)`，并检查`E(R∩G) AND overlap`是否为空。非空直接返回Mismatch，不加一个隐藏约束删去它，也不把它改名为程序fault。检查谓词是GC持久根。
4. **物理等式。** 若正常像没有重叠，则位置的第k位等于所有“owner编号第k位为1”的qi之或。所有其余坐标仍用原等式。由编码单射和同时更新，可恢复相同dense正常像。
5. **观察与不动点。** fault/abort/unsupported记录完整前态，出口记录原模型规定的后态。分支、循环合并和重复传播保持集合运算。对最终每个关系做数量与每行成员检查，再独立枚举候选物理行、解码并与oracle全行集合比较；最后canonical字节必须相等。

上述保持检查只证明本次有限可达集合，没有机械化证明所有未来语言程序。`UnsupportedMultiplicity`明确保留多活跃allocation-site incarnation的旧模型边界；未知指令、Package/Address、新helper环境或未来schema仍Unsupported。Unsupported与ResourceLimit不能计作完成。

## 独立性

`oracle.rs`从063复制，只把schema门9迁至10；它自行抽取raw事件、CFG、future-description使用和坐标，按完整原始行执行操作，不调用候选extract、encoding、BDD或symbolic转移。候选和oracle共享TypedIR数据类型、预算/容器格式及canonical序列化协议；不共享“哪一行能到达”的求解代码。

候选复用063的通用BDD、预算、顺序和遥测模块。`symbolic.rs`仍按dense语义字段构建表达式，编码模块负责读投影和物理更新。dense控制也走当前共同框架，包含身份映射及因子排序费用，因此067 dense数字不能当作063旧二进制数字。

063原29项测试作为只读模块引入：其常规`solve`便利入口在067测试中使用location；显式`solve_diagnostic`测试保留dense控制。067另加位置专属检查，包括完整相关关系、共享闭包、多重归属、raw重复give/capture、编解码、schema拒绝及完整工作量N/N−1边界。

## 成本和证据

减少位数不保证BDD减少，更不保证编译加速。当前实现仍构建dense语义schema并遍历owner/token；位置等值测试、全owner下一态保持检查和最终解码都可能增加开销。它不是已经实现的渐近O(T log C)前端，也不是生产内存管理器。

候选100M工作计入布局/编码映射、BDD构造、计划、保持检查、不动点、GC、关系计数/成员检查、节点普查、完整解码/输出和指纹。独立oracle另外记录工作和时间；生产前端另计。计时还包含候选结果与临时结构析构。冻结时写出全行JSON，正式采样同样构造、指纹读取并销毁全量字节，但不写文件。

`run.py prepare`从精确提交导出生产基线与067覆盖层，使用独立target离线构建，记录工具链、源码/输入/二进制哈希，执行有限检查和16个源码族的功能矩阵；新运行目录必须不存在，失败原样保留。`sample`只有root协调的独占窗口才能执行，2次预热加7次保留，方法循环换序，原始样本逐条落盘。任何非Complete方法不进入速度比，原始矩阵仍完整保留。

```sh
python3 tools/research/067/run.py prepare --repo . --candidate COMMIT --out out/runs/research/067/frozen-001
python3 tools/research/067/run.py verify --out out/runs/research/067/frozen-001
# 仅在root给定独占窗口后：
python3 tools/research/067/run.py sample --out out/runs/research/067/frozen-001 --window WINDOW_ID
```

具体运行、失败和结果见本轮`evidence/impl/067`及原始归档。论文启发与尚未实现的方向见[SOURCES.md](SOURCES.md)。

## Rust四目标反思

| 目标 | 本轮实际影响 | 尚未完成 |
|---|---|---|
| 安全性 | 完整观察不丢失；独立oracle、原始重复事件、正常像保持检查和预算失败均显式 | 未移入生产安全核；有限域不足以替代语言安全证明 |
| 性能 | 建立共同框架的精确表示/关系像交叉对照；全成本而非位数作为判断依据 | 无未测的编译器总加速承诺；结果需绑定冻结运行 |
| 可表达性 | 同一闭包可持有多个不同责任，相关guard与清理顺序保留 | 不扩张063支持域；返回闭包bundle和多活跃site等仍受限 |
| 工程效率 | 原模型与oracle可复用；输出字节兼容便于冷复核和反例定位 | 尚无自适应表示选择、形式证书或生产增量集成 |

本轮可检验的结构性变化是把责任关联列变成位置坐标，同时用可执行保持检查防止压缩掩盖非法多重归属。是否形成性能突破以实际结果为准。
