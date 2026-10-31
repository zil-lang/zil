# 施工11：Product默认只读捕获、跨模块trait与整数常量正确性

当前联合候选为 `c0418f532e750e7df2b00d9b7d76da82baa5086c`，PR15第二次正式CI为 [41195906918](https://github.com/zil-lang/zil/actions/runs/41195906918)。本页目前记录候选和已完成的独立检查；正式重跑与实际main验收仍待完成，不把首轮的局部门通过升级为整轮通过。

## 实施身份与有限范围

| 切片 | 精确实施提交 | 本轮范围 |
| --- | --- | --- |
| 232 | `507e650d526653ccc79146366e4bb7c30b55ac84`；原生产为 `426bbd2066e70acdb5b635ea90694c050c78f764`，后续只迁移具名夹具 | 普通局部平坦/递归Product的默认只读捕获沿真实共享地址；拥有视图读取Copy叶不误取走中间child，实际child/Managed消费保留拥有及FnOnce分类。 |
| 233 | `6d087050f232e8e8939aff297af0acc89e11bd20` | 已真实导入的公开普通trait与本地具体Self可以分处不同模块，trait库自己的泛型helper能找到该Self的具体方法依赖；生成依赖不等于源码导入授权。 |
| 234 | `32f218d419cc162f8f51e5a24a5e2fb24d50e3b0` | 在构造LLVM整数常量时先截断到实际位宽，校正窄负常量在LLVM23优化路径中的结果，并用完整i32/i64返回字节核验。 |

232新增借入仍限已证明的普通局部来源；未核参数、外层capture、动态Store窗口、逃逸和默认可变Product捕获继续关闭。真实拥有消费在创建环境时转交责任；最后一次共享使用前，相交move、take或replace继续拒绝。没有新增隐式Box、RC、锁或世代机制。

233保持名义Self、方法签名、隐私、重复实现与成员冲突检查。两个同名同布局而方法结果相差100的Self，由独立第五模块泛型库调用仍须分派到各自实现。没有开放一般trait solver、关联类型、泛型trait、lending或保留能力；未使用的保留trait声明也不能利用模块卫生名绕过检查。

234抽取自编译研究PR13的具名正确性修复，不把该研究分支整体合入，也不声称本轮证明了编译性能收益。1/8/16/32/64位等价与邻近位模式控制在LLVM整数层完成比较；完整12字节oracle另行覆盖动态i32和i64返回，避免只看进程退出码的低8位。

## 实际联合消费者

`examples/mainline/construction11`包含main、rules、data三个模块。完整Product中保存资源Ticket和data.Counter；默认只读回调两次调用trait库helper，源码完整比较结果14，再移动owner，最后stdout恰为一次`A`清理。API/Clang与O0/O2均检查实际后端、无fallback；API检查builder对象收据。

最近反例只把owner移动提前到回调最后使用之前，必须在Verify以`E-BORROW-CONFLICT`拒绝，`verification_runs=1`，没有可执行文件。正反例的原始源码、报告、输出和阶段产物均保存。首次本地把Verify计数误写为0的夹具失败也保留；生产规则未因此变化。

## 首轮正式失败及精确迁移

首轮 [41194080948](https://github.com/zil-lang/zil/actions/runs/41194080948)实际检出 `89406b33b43fc17fe967b5bb9f629a16cb44b763`，源树与候选 `77b30b4e13187ca9e5ddf8b830f11675fcaffa5c`逐字相同。结果保持Fail：默认3020/4/0，reference2976/4/0，两者零过滤且失败集合相同；其余23个步骤成功。[完整收据](../evidence/integration/construction11/attempt-1/COMPLETE.json)保存原日志、摘要和身份。

| 原失败 | 核实的原因 | 具名迁移及保留边界 |
| --- | --- | --- |
| impl072默认捕获后`give p.a` | 原合法拥有用途仍期待Unsupported | 原完整源码和exit20保持；要求完整P、FnOnce、创建环境前真实TakeLocal，并验证创建后使用owner和第二次调用都拒绝。 |
| impl077默认读取`p.a.n` | 本轮已承诺的只读捕获仍期待Unsupported | 原完整源码和exit20保持；要求Fn、真实共享P根及其地址链，活借用前move/take/replace拒绝、最后使用后move成功。其余四个原source拒绝保留。 |
| impl192默认mutable反例 | 原显式move的native步骤已成功，后续拒绝的旧诊断文字不匹配 | 仍要求Types/Unsupported；仅将文案断言更新为精确E-UNSUPPORTED及当前mutable拒绝，owned、readonly和overlap输入不变。 |
| impl077原始IR篡改 | 原描述子匹配器在新只读投影上零命中，前后Typed/Wire未变 | 保留旧0a与77b原字节对照；显式描述子控制强制命中恰好一次、模块真正改变，Analyze/raw Verify/Wire均须按原实际资源位置规则拒绝。其余共享写/FnMut攻击保持。 |

192最初被root误判为拥有捕获回归，逐行核对第278行后已纠正，未据此修改生产。077的旧无操作篡改不记为一次成功防御；真正的描述子攻击在两个基线均分别得到E-ANALYZE-IR与E-VERIFY-RESOURCE。独立对照保留Typed、Wire、命中数、前后摘要和原测试失败。

迁移候选507e650d的作者限定reference为111/0/0，非作者另行限定重放52/0/0；三个迁移正例各自O0/O2的六次运行保持原exit20。无默认功能下零项的impl225_machine明确排除覆盖。两套重放不相加为新增独立测试。生产、工具链和规范字节与首候选相同，第二候选只增加五个具名测试/审核路径及记录。

233更早的首候选 `41d136fa31cc00b5a4b81f3c03092ee31f0d6f00`也保持独审Fail：trait定义模块内泛型helper丢失具体实现方法依赖。6d087050返修后，原七组逐字重放与新增名义身份/真实import反例通过。该缺陷及返修不因后续通过从历史中删除。

## 预算与实际证据边界

232旧legacy Type/Analyze/Verify为835/7129/12161，225为3889/36339/56097，226为1787/15791/30761，均逐值保持；新flat为1226/7896/16608，recursive为1788/14756/29096，owned为1762/12234/17419。233旧242/1568/1432/1412保持，新counter最小16012、trait library最小2003。N、N−1、cold/warm/fresh与失败后恢复按各自固定输入核验，没有提升默认额度。

首轮234/joint所选原生证据已由两位非作者分别核对。234包含24份作者编译报告、88次运行观察，以及8份独审编译报告、56次完整12字节观察；8组API冷暖配对核对action key、builder/cache、当前源码Verify与无fallback。builder宽度控制的40次执行依据冻结循环和成功CI日志，逐例临时对象由原测试清理，不能冒称都留下了原件。它们仍只是首轮失败运行内的限定通过证据。

首轮State/分配失败消费者为16次构建、40个API对象、128次执行观察、88次分配观察和56次实际故障注入，全部观察live=0且allocated=frees。fmt、Clippy、text-reference37项和迁移runner9项按原件核对。最终验收必须另外绑定第二轮及实际main的完整结果。

## Rust四目标与后续义务

有限来源、所有权、真实清理、模块身份和raw/Wire边界有本轮实证，完整语言安全仍未建立。真实原生输出与分配平衡没有证明同语义Rust运行、RSS或尾延迟同级；源组合能力增加也不等于完整维护成本优势。预算不提高不等于编译更快，本轮没有Rust性能配对实验。四目标合取和产品级可重复净突破仍未建立。

完整1.4/ZUI保持NotSatisfied，B0 Doing、181施工，产品与原生Windows为NotRun；schema/Wire27和产品阈值保持。ZUI继续用于识别语言前置，本轮未开展框架开发。

下一最小前置是227普通同步defer的独立合同：注册成功后，在正常落出、return、`?`、break、continue等承诺路径恰好执行一次。FnOnce只保证至多调用一次，环境销毁不能代替动作履行。必须明确注册/履行状态、真实作用域激活、借用保持、返回值准备及与局部逆成功构造顺序的交织，再决定公共IR/Wire与缓存版本；该项目前仍是未实现义务。

历史handoff记录完整性仍保留1145项缺口；最终收口会重新执行并比较错误集合。该记录门与编译器CI分开。临时分支与工作树只在实际main成功CI、精确tip及祖先关系守卫通过后整理；当前尚未执行最终清理。
