# 已完成 N64 多文件 after 请求的只读成本诊断

结论：**一处 leaf 文件中的两个 body 改动，确实同时使自己的原生分区和覆盖全部 leaf 的入口分区失效；后者重新执行完整 LLVM O2 与机器码发射。** 这不是根据总耗时推测，而是当前 recipe、seed/edit 对象 key、逐 unit receipt 和未改变的 partition 源码共同证明。a60 优化的是 layout key/recipe 复用，没有改变这一完整调用闭包导入结构，因此不能消除主要原生重编成本。

只读数据范围为 `runs/inc-after-streaming-a60/rows.jsonl` 中已经完成的 N64 全部184行，均 valid；暖CLI每个k的A/B各4请求、Rust各2请求。正在继续的其他规模不纳入本结论。完整提取、逐unit均值、实际路径和解码后的recipe保存在 `n64-cost-diagnosis.json`；其中记录读取时rows快照SHA256。没有编译、测试、改源码或新增采样。

## 实际失效范围

| k | 实际变更文件 | seed→edit对象key改变且builder重编的unit | cache命中unit数 | object-patch |
|---|---|---|---:|---:|
| 1 | m1 | 0、1 | 6 | 0 |
| 4 | m1、m17、m33、m49 | 0、1 | 6 | 0 |
| 16 | m1、m5、…、m61 | 0、1、5 | 5 | 0 |
| 64 | main及m1…m63 | 0…7全部 | 0 | 0 |

以上模式在每个k的4个A与4个B暖CLI观测中都相同，不是挑选单次结果。直接逐位置比较 seed.stdout 与 edit.stdout 的 cache.action_key，改变位置与builder位置精确一致。resident现有N64格也呈相同对象失效模式。

文件owner并非“一文件一个对象”：machine_cgu.rs 按 `span.file` 分组、每256函数分chunk，再按 `(file % count + chunk % count) % count` 分配，当前每leaf只有9函数，故 file_id%8 决定其canonical owner。于是四文件1/17/33/49全落unit1，十六文件全落unit1或5；k大小与失效对象数不成线性关系。

## 大入口 unit 的直接结构证据

对B的 `n64-edit-1-warm-cli-b0-p2-B` 已保存 v2 layout文件解码：其72字节header后为原Layout::encode的u32 count/owner/四组indices。program.ll是partition前machine.items按原顺序输出的精确诊断文本，能将indices映射回完整定义；并非假定program.ll本身已经是unit0。

当前机器模型共有577个item，其中572个函数定义、5个声明。布局为：

| unit | canonical definitions | 导入的完整函数body | 实际定义body合计 |
|---:|---:|---:|---:|
| 0 | 68 | 504 | 572 |
| 1…7（每个） | 72 | 0 | 72 |

unit0 canonical部分含main文件的own_a/own_b/value/main四函数、入口wrapper，以及file_id为8/16/…/56的7份leaf×9函数。其余56份leaf的全部9函数通过完整call closure导入，共504；两者合起来覆盖**全部63份leaf**，加main四函数和wrapper，恰为572。

具体m1映射为h0=`zil.fn.6`、h7=`zil.fn.13`、value=`zil.fn.14`；leaf i对应ID `6+9*(i-1)` 至 `14+9*(i-1)`。该映射可由当前source声明顺序、main.value的63个leaf.value直接调用、每份leaf.value的8个helper直接调用与report.source_files相互核对。unit0 recipe明确imports含6/13/14；unit1明确canonical definitions含相同符号，故m1两body变化同时改变两个unit的实际完整机器内容。

原因在源码：ResourceHelpers::prepare先要求非空lir.closures，本fixture只有具名普通函数、不含资源闭包，不能采用其小源函数导入路径；ordinary partition随后按direct_calls遍历完整传递闭包，跨owner的非foreign source函数只要满足External/local-identity/全图复制预算就全部导入。Layout::replay把这些body命名为 `zil.import.0.<canonical-name>`，只重写直接调用，canonical地址原样保留。main.value调用每个leaf.value，leaf.value又调用8个helper，因此入口闭包实际包含全图。

## LLVM 的实际时间

暖CLI均值，单位ms；这些是完整请求与其当前原生receipt，非新增诊断采样：

| k | A完整请求 | B完整请求 | Rust完整请求 | A unit0 optimize / emit / total | B unit0 optimize / emit / total |
|---:|---:|---:|---:|---|---|
| 1 | 957.207 | 975.956 | 267.819 | 415.369 / 310.852 / 762.877 | 403.849 / 303.629 / 780.896 |
| 4 | 906.619 | 934.676 | 463.966 | 389.056 / 320.804 / 740.534 | 408.314 / 317.614 / 763.389 |
| 16 | 1090.000 | 960.804 | 628.116 | 482.228 / 333.331 / 863.548 | 436.253 / 319.579 / 792.910 |
| 64 | 1269.244 | 1127.594 | 627.870 | 491.753 / 407.472 / 932.712 | 404.670 / 365.138 / 810.322 |

最直接的一次B观测（k1/b0/p2）中：unit0 preflight11.847、builder15.823、LLVM optimize389.604、emit296.963、total717.621ms；unit1 optimize41.245、emit99.675、total144.657ms；其他6个unit为object-cache，仍执行当前完整preflight。没有任何object-patch。

源码 native.rs 的optimize_ns明确围绕实际LLVMRunPasses(default<O2>)计时；emit_ns围绕LLVMTargetMachineEmitToMemoryBuffer计时，包括机器码生成、对象发射及取出内存buffer。因而不能把全部耗时笼统称为“LLVM optimizer”：k1 B unit0四样本中optimize均值403.849ms，emit均值303.629ms，两段在同一unit中顺序执行，合计707.478ms；unit0 total780.896ms已相当于完整请求975.956ms的约80%。

build.rs把最大cost unit优先排入两个worker，unit0的长任务与其他unit并行。全部unit的total/optimize/emit求和是跨worker时长之和，**不能从完整请求wall中相减**，也不是独占CPU。JSON同时保留进程CPU和每unit时长，未混为同一指标。

## 为什么没有局部 object patch

事实是所有当前receipts都只出现builder/object-cache，没有object-patch，也没有记录具体早退reason。源码能确认至少两项适用域限制：unit0导入函数同时保留canonical `zil.fn.*` 声明，而Snapshot::new显式拒绝这种声明，且要求可patch函数完整External签名，内部import clone也不满足；因此大unit根本不进入该窄patch域。

普通leaf unit即使有base snapshot，两helper变化的reverse dependency closure会加入同leaf.value；而value还引用另外6个未变化helper。Snapshot::plan要求patch内当前函数的所有定义callee也在patch中，因此这个3函数集合不闭合，足以拒绝该派生路线。这里是根据当前调用图和源码得出的确定限制，不声称receipt记录了某个唯一first-failure reason；其他预算条件也可能更早拒绝。

## a60 的边界与仍缺的计时

所有N64 warm/resident格里，A的layout文件集合为seed后1份、edit后2份；B保持同一份v2文件名与内容哈希。说明B当前body编辑保持布局投影身份，未再生成新的recipe；对象失效位置、unit数和完整LLVM策略则完全相同。现有report没有显式layout-hit标志或layout key/planner分段计时，不能仅凭文件存在量化省掉了多少毫秒。

特别不要把object_cache.key_ns当作layout key耗时：它是对象完整机器内容key，k1 A/B均值约11.532/23.563ms（原始JSON可复算），与layout投影不是同一阶段。当前暖CLI每次仍记录parse executed64次，以及expand/ct/types/analyze/verify/lower/link各一次；但query_events只有次数，没有这些阶段的独立wall。语言Verify、Lower、partition、诊断IR、链接、发布各自占比仍无法由现有计时精确拆分。

因此当前可以明确指出大入口的重复全图优化/发射是实测重成本，也能确认a60没有减少它；不能把N64 k64的A/B总差额全部归给layout缓存，不能把Rust差距全部归给Verify或解析，更不能据此降低O2、当前Verify/preflight或运行质量门。下一实现选择应针对这条已证实的重编结构并保持原合同；本轮矩阵保持原样完成。
