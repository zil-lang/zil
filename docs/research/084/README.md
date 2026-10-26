# 084：利用无悬空依赖不变量，避免新键发布时全图轮扫

当前状态：冻结两版功能对照与正式采样完成，**生产NoGo**。六行shortcut已经恢复为
生产基线；差分测试、原候选历史和完整正负证据保留。结论与归因见[RESULTS.md](RESULTS.md)，
全部48个请求单元及12个进程单元见[TABLES.md](TABLES.md)。
候选身份 `b555a26bc3f3e9d3b9459b04bb60bfe4025a39f2`，收据见
[functional.json](../../../evidence/impl/084/functional.json)。66个guard请求逐版相等；
12个模式单元的96次完整请求、48组配对和24次原生行为全部通过。每请求独立Verify；
analysis预算边界3122/3121、CT预算边界12/11在两版一致，包含同会话失败与恢复。

被测候选只修改 `zil-db::Database::invalidate` 的六行：当被失效的key不在快照表中，
立即返回空集。依赖只能在输入已发布并通过完整传递读取检查后保存，删除操作又会
删除完整消费者闭包，所以可达数据库没有指向缺失节点的边。缺失key没有消费者，
提前返回与原轮扫结果相同。同key替换、五种观察指纹、传递失效、活动查询、取消及
失败规则保持原实现。详细证明前提和最近反例见[实施前提案](../../../changes/IMPLEMENTATION-0084.md)。

候选没有新增反向索引，没有把指纹命中当核验许可，没有修改Analyze/Verify，也不缓存
Verified能力。这是已有图不变量在生产成本上的小型工程应用，不是原创图切割复杂性定理。
无入边时不必搜索消费者，与物理局部相互作用的直觉相容；正确性依据仍是明确API不变量。

## 冻结身份和复现

基线为 `f95cdd940ca4c41490738c75144c6538c27c295c`，draft17/schema14，包含068和080。
并行主线schema15不进入这个冻结实验。每次prepare都导出基线/候选的实际源码和相同
runner，记录逐文件SHA256、工具链、profile、二进制、输入与精确commit。

```sh
python3 tools/research/084/run.py prepare --repo . --candidate b555a26bc3f3e9d3b9459b04bb60bfe4025a39f2 --out NEW_DIRECTORY
# 只在root授予独占窗口后运行：
python3 tools/research/084/run.py sample --out NEW_DIRECTORY --window ROOT_WINDOW_ID
```

两个版本均为release、codegen-units=1、debug=0、incremental=0、jobs=1。功能检查的dev/test
也关闭debug与incremental以避免重复缓存膨胀；功能准备的计时不晋升性能证据。运行目录
保存全部报告、命令、stdout/stderr、失败和预算尝试；不得覆盖旧目录或只保留成功样本。

## 控制范围

DB单测保留仅测试编译的旧全图扫描。64个四节点DAG、五种边种类轮换、五个存在/缺失
producer和六种改变/不变结果，共9600个可达图控制；含重复边、删除后重新发布和真正
同键替换。另核活动祖先、无begin/失败/取消/基础设施失败发布、clear及既有磁盘完整性。

完整生产控制runner另核CT依赖编辑、已调用私有写效果编辑、插入造成ID重排、target、
构建输入、文件身份、诊断源位置、token限额、取消前/命中后/发布前与恢复。对实际完整
请求二分求出analysis和CT预算N，并执行N、N−1、同会话恢复；两版完整报告和阶段产物
必须相等，不能把ResourceLimit计成成功。

9个语料、12个模式单元均通过真实compile_path入口：

| 语料 | 请求模式 | 容量 | 编辑与目的 |
|---|---|---:|---|
| 标量 | Verify | 1024 | 被调用private常量改变；公开函数新增参数 |
| 16/64/128/512导入模块 | Verify | 1024 | 所有模块都被真实引用；m0私有正文改变，再改公开参数并同步根调用 |
| 64模块默认容量 | Verify | 32 | 明确覆盖现有请求开头整缓存清空；不把它称为热命中 |
| owned_component / helper_ui | Verify、Build | 1024 | 私有digits函数改变实际输出；公开number新增参数并同步调用方 |
| byte_io/arithmetic | Verify、Build | 1024 | 被调用u8运算分支改变checksum；公开number新增参数 |

每进程四个请求为cold、unchanged、private_edit、public_edit。private edit保持公开签名
指纹且改变真实程序输出；public edit必须改变公开签名指纹。大容量unchanged须复用raw，
默认容量控制必须确实清空并重执行。每个成功请求Verify次数必须为1、活动查询数为0。
功能准备打开阶段产物并逐字hash对照，Build再核对固定预期的原生stdout/stderr和退出码。
正式采样关闭未请求的阶段dump，不省任何实际检查或发布成本。

## 成本解释

计时包括完整compile_path API、报告时间和完整进程wall。进程范围还包括实际输入摘要
记录、报告输出、编辑恢复和会话销毁；源写入发生在API计时外，读取、发现、解析、查询、
指纹、失效、Verify和Build外部编译链接都在API内。cold只表示空编译会话，不清OS页缓存。
Linux VmHWM为四请求整个父进程高水位，不含clang子进程峰值，不能声称总编译峰值内存。
固定1轮预热和7轮配对，先后顺序交替；bootstrap区间只描述单机有限样本。

正式窗口`draft18-084-exclusive-001`已经完成192进程/768请求，没有补样；完整逐次
观测见[performance-samples.json](../../../evidence/impl/084/performance-samples.json)。
64模块unchanged也观察到改善，而它不执行新键发布，因此该改善不能计为shortcut收益。
真实应用净收益未建立并出现不利观测，最终不把此优化投入生产。

假设可能被实测否定：真实小项目的失效轮扫份额很小，删除这部分工作也未必形成可辨认
的完整请求收益。完整反向索引之后必须对本简单控制比较，而不是只对更弱的旧轮扫取胜。
本片无同能力Rust成本比较，不改变产品门，也不把未测的RawSnapshot哈希和clone说成热点。
下一轮的预算解耦、发现器复用和结果投影实验详见[NEXT-EXPERIMENTS.md](NEXT-EXPERIMENTS.md)。

## 下一条纵向研究线索

[Rust官方增量查询说明](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation-in-detail.html)
介绍 `no_hash` 与projection query：避免大单体和投影重复计算结果摘要，由小投影判断
下游真正需要的变化。这可为ZIL的Debug字符串摘要另立协议实验，但084没有实现它。
若改用保守输入身份或不哈希单体，须明确结果身份、位置、限额、公开合同和私有正文依赖；
不能直接把QueryKey换名当作原结果指纹。该文核对日期为2026-10-25。
