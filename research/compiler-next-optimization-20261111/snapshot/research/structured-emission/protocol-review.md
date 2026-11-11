# 直接结构发射：84请求screen协议静态复核

结论：**以固定诊断screen身份，当前冻结协议无运行阻断。** 本复核者是sink源码作者，不是该源码的非作者验收者；本次独立于既有measurer实现进行只读协议核查，没有改screen、模型、源文件或plan，没有运行编译/native/采样。源码的非作者静态独审另见同目录`source-review.md`，不能用本报告替代。Root统一性能窗口执行一次；本报告不代报任何新测量结果。

## 冻结与归档对照

| 对象 | SHA-256/身份 |
| --- | --- |
| `screen.py` | `301a2fa5a6e4a0947f8664f67ce4b2049a1a9ef72635fc32d1847e85360edb0a` |
| `corpus.py` | `2dfb26675a78e1ad83d72c20fa85d8c8bf8d1bf0d7b3db3ab0e84bf4be994a78` |
| `out/next-optimization/structured-ee0-plan.json` | `6fb22be8b6c199e40cb7f6a2370f6ea7b9055c0f02d81ad7a92cf48acda5ff2c` |
| baseline源码 | `99035931d358add0eaed6d4535035c38413701dd` |
| candidate源码 | 发布`ee0cc45261dbfadf25dce567f0fc1fa0b4d4b6f5`；作者local `924076ca11c75e88bb1bcb9df98c58d0dae1bbd2`，Root核同tree `ad37913cf49a1bfec28e66e7abdcf225dbcafed5`，不称同commit |
| baseline binary（plan pin） | `64e1ff43bbaa59f786f1cc899b69e707072219a9c0abf0122ce31cd7b77f981d` |
| candidate binary（plan pin） | `adf69c444bb689e961f7dc404d75e6da0f7aec1e597b3f557919cdaf0bce8043` |
| clang binary（plan pin） | `c336d48383874cffaab0dc4c5c359456d871f21380b746520dde5c7f4c125a67` |

静态读取已有raw归档中的`raw/multi-edit/original-controls/{layout-screen-plan,streaming-screen-plan}.json`，确认两份旧plan的script/model hash以及六份源文件hash均与当前相同；与streaming旧plan的完整pairs、oracles、configuration也逐项相同。未执行旧script或重新生成fixture。新plan已显式覆盖script内陈旧默认source IDs与旧工作目录，因此没有把上一轮worker候选误标成本轮源码。

## 固定覆盖与计数

| Lane | 每arm请求 | pair数 |
| --- | --- | --- |
| shared4096/jobs2 | cold1、warm7、private1、edit-warm3 | 12 |
| shared1024/jobs2 | 同上 | 12 |
| owned8/jobs2 | 同上 | 12 |
| shared4096/jobs1 | cold1、warm3、private1、edit-warm1 | 6 |

合计42对、84次完整编译、252次短native I/O。pair类型为cold4、warm24、private4、edit-warm10；固定seed20261111产生base先21对、candidate先21对，之后交替，未观察结果后挑顺序。固定CPU `[0,1]`，两arm都继承相同affinity；jobs1仍在相同两CPU预算下只请求一个codegen job，不是另一硬件配置。请求均为fresh CLI，语义/object cache按case/jobs/arm独立；先base快照cold与warm，再在同一arm目录切private快照并测private/edit-warm。

命令显式`--backend llvm-api --opt O2 --codegen-units 8 --incremental-o2 --function-merging after`，显式clang路径及semantic/object目录。这里的“cold”是此lane/arm首次编译且cache目录新建，不代表清OS页缓存，也不代表禁用所有缓存。该screen不覆盖daemon、不覆盖多文件批量或并发候选。

## 计时与质量核验

- timed的wall从启动Popen之前到wait4结束，包含fresh CLI完整构建、链接与发布；source复制、输出unlink、hash/日志复制、短native检查在计时之外。这与旧screen的完整请求边界一致。
- CPU记录wait4返回的user+system usage，RSS记录其`ru_maxrss`。这是操作系统该child作业的usage/max口径，**不是并发进程树同时RSS之和或树峰值**；不能把它与新造的树RSS指标直接比较。无硬件频率/热状态控制或背景任务隔离，依靠Root独占重窗口和配对顺序减少干扰，单次短screen不是统计稳健的总体结论。
- 每个请求要求CLI成功、`verification_runs == 1`、当前源hash相同、object cache enabled且无corrupt/write failure；warm/edit-warm强制全部当前unit命中、miss0。实际units/jobs/fallback/hits/misses/stored等必须跨arm一致。cold/private没有硬编码预期miss数，保留完整原receipt，不能只看summary就声称命中拓扑达到另一个设计目标。
- 42对完整ELF bytes必须相等。sidecar原字节及副本都保存；material比较只删除第二行且唯一的`; verified-fingerprint: `注释，其他任意差异失败。不能把raw sidecar相等与material相等混用。
- 每请求以3个固定stdin（`01000001`、`1d020002`、`fb050003`）各跑fresh executable，对独立Python model的完整stdout、exit0、空stderr核验。计数252是短功能核验，**不是252个运行性能样本**。
- script没有逐条强断言object_reports的preflight/LLVM pipeline字段；它保留完整native_codegen供后核。当前未改生产`build.rs:787`在object hit执行完整`validate_module`，miss走`compile_module_with_function_merging`；`native.rs`的Structured输入先完整preflight，原LLVM before/after Verify和default<O2>保持。故“保留预检”依据当前精确源码及raw receipt，不能把screen中Verify=1等同所有LLVM检查均在当前hit上重跑。

## 失败保留与解读边界

prepare拒覆盖plan/out；run新建exclusive attempt sentinel并复制plan，冻结校验script、binary/clang、model与sources。普通编译失败、300秒超时、spawn错误、坏JSON、缺字段、拓扑/ELF/IR/native不等均保存失败且令complete=false；不得把缺格排掉后称84通过。summary的cell均值仅纳入valid rows，所以必须先检查complete=true、84 valid、42 pairs、252 I/O及三种跨arm一致旗标，再解释均值。中断消耗attempt，不允许修改plan或补重复。

沿用原measurer的失败边界也保留：被外部中断时，当前尚未完成的timed调用可能仅留下stdout/stderr，未返回的row CPU/wall不会凭空补齐；最终summary保留已完成rows并标interrupted/incomplete。host记录写入发生在测量try之前，若该准备阶段失败，attempt仍消耗但可能没有completion。不能把这些情况称完整或自动重试。当前plan已单独冻结SHA，Root应保持plan和工具文件不变，不因run只起始校验便认为它们天然不可变。

该screen不含Rust arm、runtime性能、原始八项四频率加权或新660/378请求矩阵。即使全部同码且编译明显更快，也只支持进入后续正式门的判断；不能替代原始总体加权、运行≥Rust90%、相对运行回退≤10%或完整workspace/reference/text门。script里旧机制标题只是复用原字节的历史标签，本轮机制身份由新plan、源码tree与binary pin绑定。

## Builder新fixture的静态调度推导

对象为独立候选`78e300443e162bbb963b809e1066b9bd08197a77`、tree `257a29612531576af3f9fcf1f703fb824970f695`。没有构建或native执行。

依当前`preflight.rs:1007–1067`逐条枚举非Phi位置、去重SSA/块内依赖并按FIFO ready队列，fixture的instruction IDs为：entry 0–4（alloca/store/load/compare/branch），left 5–7（add/add/return），right 8–10（constant-add/add/return）。初始ready `[0,8]`。依赖集合为0:{}，1:{0}，2:{0,1}，3:{2}，4:{3}，5:{2}，6:{5}，7:{6}，8:{}，9:{2,8}，10:{9}；条件值的块内sequence边与数据边去重后如上。

因此严格推导schedule IDs为`0,8,1,2,3,5,9,4,6,10,7`，对应block indices `0,2,0,0,0,1,2,0,1,2,1`。这是确实交错且返回既有block的fixture，并含entry连续链和right开头constant folding；11条schedule指令、9个连续block runs，只应省2次schedule定位，不夸大为每块一次。Phi预建遍历仍对3块各定位一次，此fixture本身无Phi，Phi回边依赖现有loop/forward-block控制。以上是静态公式结果，不是已经执行或通过的证据。
