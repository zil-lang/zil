> 2026-10-31 开工只读审计的冻结记录，保留原分支/SHA/当时路径作为来源。本文不是当前实现状态；已修项目和最终实测见 ../SLICE1.md、../SLICE2.md 与 ../RESULTS.md。

# 前端和增量审计（2026-10-31）

审计对象：main `21058d0e331de764e3a1b9123c3e1edf5e55a1aa`；重放对象：compile7 `4b9ae3826a2d60102dcbb68f01bb76db5ea0532f`。没有修改源码。当前开发profile仅用于工作量/缓存事件反例，不作性能成绩。

## 已执行反例

`incremental_probe.rs` 链接两版真实 zil-driver rlib；`check_incremental_probe.py` 核验两版18条观察断言。完整报告在 `incremental_probe_{main,compile7}.jsonl`，摘要在相应 `_summary.json`。

| 请求 | 两版共同结果 |
|---|---|
| 28模块同Session冷→完全未变暖 | 32查询全部复用（28 Parse + Expand/Ct/Types/Analyze） |
| 29模块同Session冷→完全未变暖 | 33查询，0查询复用 |
| 40模块同Session冷→完全未变暖 | 44查询，0查询复用 |
| 4模块，capacity=1000，首函数return 0→return 1 | 仅3个未改Parse复用；整项目Expand/Ct/Types/Analyze重做 |
| 同图首函数return 1→return 1+0 | 0查询复用，连后3个未改Parse也失效 |
| 29模块真实import文件图compile_path冷→暖 | 0查询复用；loader可能在清缓存前借入既有syntax，不将其误称为所有Parser重跑 |

所有请求都Success、verification_runs=1，无忽略检查。该探针显示失效形状，不等于真实项目端到端加速比。

## 最重要的结构问题

1. **整体项目身份支配几乎整个前端。** main `project.rs:157–166` 构建项目key，226、295、366分别缓存整个Expand/Ct/Types，`session.rs:170–187`实际依赖全部PrivateImplementation。接口有signature/implementation fingerprint，但尚未把这些结果接入语义依赖传播；一个私有body即整项重算。compile7保持此入口结构，另有query-cache研究入口需独立限定其支持域。
2. **默认Session容量悬崖。** main `session.rs:77–87`默认32条；`project.rs:63–65`及`session.rs:118–120`超限整表clear，按条数而非持有内存收费，且保留多历史源版本。一个29模块请求本身已超阈，完全未变请求每次也丢语义缓存。compile7同样`session.rs:134–135`/`project.rs:78–79`。cp32分支新增独立type memo及bounded retention，raw查询仍32，并不等于该悬崖已根治。
3. **预算成为伪语义依赖。** main `project.rs:179–185`、773–785将前模块消费后的remaining token/node quota带入下个Parse key；`session.rs:575–588`把全部limits纳入。前模块多几个token，即使后文件字节完全没动也miss。cp33分支已出现`parse_identity`/`fits_limits`收据方案，不能把这项说成无人解决；本次尚未实跑cp33。
4. **身份不稳定使对象缓存难救。** main `project.rs:831–850`按发现顺序赋ModuleId/FileId；`zil-types/src/lib.rs:145–152`FunctionId=offset+i，`program.rs:92–105,117–124`全局顺序拼接。插入声明/导入可能改写不相关后续身份。应把局部紧凑索引与稳定声明/实例key分开，诊断位置作为当前快照侧表；不能拿字节偏移作稳定身份。
5. **CLI不是持久前端。** `zil-cli/src/main.rs:225`调one-shot；`zil-driver/src/lib.rs:245–251`每次新default Session。`--cache-dir`仅校验/写源字节（`session.rs:619–644`），不复用解析/类型结果。IDE持久EditorSession已有（`zil-ide/src/lib.rs:9–43`），便利函数仍one-shot（46–53）。后端对象缓存与完整前端增量是不同能力。
6. **查询管理本身有可避免复杂度。** main `zil-db/src/lib.rs:150–188`每读重遍依赖闭包；`publish`调用`invalidate`，后者249–275反复全表扫描直到闭包闭合。新叶query发布也全表扫描；n条Parse出版累积至少二次型表访问。cp21/cp23/cp30已有对应实验，父代理需按分支结果区分修复与剩余。
7. **Verify/Lower/文本仍设未变请求下限。** main `session.rs:416–418`深clone Typed/Candidate进入独立Verify，445全Lower，528读回完整LLVM文本；cp35已研究共享输入。每请求Verify是安全契约，深copy不是（设计1112–1120明确允许共享）；必须避免将跳过独立核验当加速。

## 更有价值的突破线路

先做一个真实端到端“单项修改局部重编译”垂直切片：stable Def/Instance key → header/name lookup查询 → 一般函数Type/合同查询（真实递归SCC）→ 独立Verify own-query/证书检查 → 单项Lower → 稳定CGU/object/link。别再用全项目SHA当其中每层key。可先从单态标量/普通借用函数覆盖已有支持域，不把未用定义从检查集合删除；复杂泛型/CT不支持细粒度时走保守旧路径并单列覆盖率。单纯引入daemon/Salsa不是这个切片的替代品。

独立核验可以借现有“候选/核验”架构转向归纳证书：Analyzer产生块不变量/函数效果证书，Verify逐入口、边、转移和安全义务独立检查，减少重复固定点搜索；测证书生成/大小/哈希/核验总成本。现有规范1118–1120明确允许研究，生产替换前与旧独立重算差分。即使这样，每请求全证书扫描仍有线性地板；真正增量Verify应拥有自己的受信依赖图和不可变输入，而非反序列化旧Verified或相信Analyzer命中。

最便宜证伪实验：n/2n/4n模块×私有体等token变更/增token变更/公开合同变更/布局CT变更/插入未用声明。报告source字节读取数、Parse次数、函数check与SCC次数、Verify边数、Lower项数、CGU命中/链接、分配字节和全成本，不只hit率。查询淘汰应按保留字节和live最新epoch分开记账。冷与热要一起测，冷成本恶化不能靠重复no-op权重掩盖。

## 实验边界与 compile7 定位补充

API计数里的28/29/40是 `ProjectSources.modules.len()`，root是其中ModuleId(0)，每模块一个普通函数。该API允许调用者显式供应多个模块（本探针无import边）；调用的是同一个 `CompilationSession::compile_project` 实例，options为default（停在Verify）。额外真实文件实验是root/main加28个导入模块，共29模块；调用同Session的`compile_path`。两者均为生产公开入口，不是手填TypedIR。缓存计数是RawSnapshot查询条目数，不是函数数。

compile7 中相应定位：`project.rs:200–209`项目指纹/key，321/390/475三个whole-project查询，481全项目check_program；`session.rs:195–210`完整PrivateImplementation边；`project.rs:964`顺序模块ID，`zil-types/src/lib.rs:148`offset+i函数ID；`lib.rs:272–273`新Session；`session.rs:463–465`Verify深clone，497全Lower，586读取LLVM文本。数据库 read/invalidate 行号与main一致（150/249）。其目录为 `/workspace/.zil-env/research/compile7`，来源精确SHA见开头。

文件入口load_sources在请求clear前可以借出Parse的Arc，因此filesystem_29_warm=0是**查询发布层无复用**，不意味着该请求词法/语法函数都重新执行。此处确证损失的是Expand/Ct/Types/Analyze复用；API入口没有该loader借用，query事件与实际parse执行一致。本次未将观察推算为墙钟倍数。
