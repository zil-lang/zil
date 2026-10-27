# 源码、审计与历史交付

## 语言169起：源码、证据与恢复附件分离

2026-10-27用户的新要求适用于后续工作，上一份161–165交付不重打。新主源码ZIP从干净main的Git跟踪文件逐字导出；保留源码、测试与输入夹具、规范、配置、锁、工具、文档和任务记录，排除整个evidence目录、构建输出、缓存和Git数据库。跟踪的脚本按原执行位保留；生成的原生程序、对象和LLVM执行产物不进入主包。文档内的历史证据链接在源码包中可能不存在，必须按对应历史交付查阅，不能当成本轮重跑。

当前入口：

此外，工具明确排除文档目录中13项历史生成收据/测量图与handoff生成快照，精确名单在 `tools/language/169/package.py` 的 `EXCLUDED_FILES`。测试输入、JSON合同、规范登记、源码与普通文档保留；不会按扩展名粗略删除所有JSON。生成快照缺失时，精简源码目录不具备历史handoff所需的完整身份，需按下述恢复方式取得Git checkout。

```sh
python3 tools/language/169/package.py --output ../delivery/zil-language-source.zip --audit-output ../delivery/zil-language-audit.zip --receipt evidence/impl/169/wire-02/command.json --receipt evidence/impl/169/wire-02/output.log
```

审计恢复ZIP只添加明确选择的紧凑文本证据与当前全部可见refs/分树HEAD的Git历史bundle；默认总上限50 MB，超过即报错，不静默丢弃。没有选择的旧运行不声称重新交付。Git历史原有字节不改写，历史对象可能含旧轮已提交产物；当前新执行产物不单独收录。主包的PACKAGE.json记录来源提交、crates树、当前规范摘要、逐项外置路径和审计ZIP哈希；SHA256SUMS与原有package_check.py兼容。

主源码可直接构建，不依赖证据。需要延续Git历史时，先解压审计恢复ZIP，再执行：

```sh
python3 history/restore.py --manifest history/manifest.json --destination ../zil-restored
cd ../zil-restored/main
node tools/handoff.mjs --check
```

恢复检查核对全部refs与提交身份；它不把继承的历史交接问题变成Pass。无Git源码包仍不能通过历史handoff。实际语言测试、原生行为、恢复结果及各文件最终大小，只以本轮最终报告和各自精确提交的收据为准。下文各draft说明属于历史交付格式。


## 2026-10-26 draft24检查点

本轮前缀为`zil-draft24-20261026`，包含完整非evidence源码与紧凑收据、相对原draft22完整历史的累计history-delta、累计evidence-delta、恢复脚本、验证和报告。仍使用`RECOVERY-PACKAGE.json`表明开发检查点；当前完整正式门NotRun，不把历史35项正式门继承到本轮。

累计增量已经包括120–127及独立审核、图研究原候选/净回退和全部已命名refs；应用时只需要原draft22基础历史与本轮增量，不必先串接上一轮draft23增量。源码ZIP可独立构建，继续施工用restore.py核全refs/提交集合、文件内容/模式和fsck。独立冷恢复从空target执行本轮124–127五个测试目标及两个真实多模块消费者，不重新采样性能。实际完成状态以旁置报告为准。

实施结论见[本轮语言与图研究](DRAFT24-LANGUAGE-GRAPH.md)，其他模型接续见[外部研究交接](research/EXTERNAL-CODEX-HANDOFF.md)。保存链接与每份SHA在最终文件索引，未推送未指定Git仓库。


## 2026-10-26 恢复检查点

当前交付使用 `zil-draft23-recovery-20261026` 前缀：source.zip、history-delta.bundle、evidence-delta.zip、verification.zip、report.json、report.md、restore.py及SHA256SUMS.txt。真实生成/冷恢复/保存成功以最终旁置报告和回执为准。

源码包使用独立 `RECOVERY-PACKAGE.json` 清楚标明开发检查点与尚未重跑的完整正式门，不覆盖旧PACKAGE格式的完整审计语义。新历史是相对已保存draft22完整bundle的增量；需要原三个历史分片还原并核对SHA后再应用，不能把增量单独当成完整历史。evidence-delta只包含相对基线新增或变更的已跟踪证据；原审计仍按原范围保留。

恢复与程序验证见[本轮报告](DRAFT23-RECOVERY.md)。旧draft23不可恢复提交不重标已保存；120/121/122依据新源码重新实施和独审。未推送任何未指定Git仓库。


## draft22 历史交付入口

本轮七件按`zil-rust-0.1.0-draft22-20261025`命名：`-source.zip`、`-history.bundle`、`-audit.zip`、`-verification.zip`、`-report.json`、`-report.md`、`-SHA256SUMS.txt`。实际生成、冷恢复与保存状态只读最终旁置报告及真实回执；本文记录交付合同，不预填执行成功。

生产schema/wire保持17，沿原Git历史继续。101–110的候选、修复、独立审核、正式判定和生产回退均保留；101/107的局部收益与Inconclusive、108的NoGo不会被后续结果覆盖。历史bundle包含封口时全部可见refs和全部worktree HEAD的提交闭包，不能将reflog-only或任意未跟踪目录也称为已保存历史。

源码ZIP可独立构建；续接施工同时使用完整history.bundle。审计ZIP包含本轮明确选择的完整raw：104的时间/行为/目标RSS与首次无效系列，101/107/108/109的完整归档、语言片作者和非作者反例、Roam差分及最终统一门。每个逻辑原件均有路径、长度与SHA；109标准tar内的重复载荷通过安全的向后直接hardlink保存，恢复时必须核对全部逻辑成员，而不只核对首次出现的内容。旧轮外置audit继续承担各自历史范围，不声称重新收集所有旧未跟踪运行。

正式受测提交、只追加说明/证据后的打包HEAD及独立冷恢复身份分别记录。冷恢复核对完整refs/提交集合及每份源文件与Git的字节身份，从空target构建；实际运行110迁移后的16个旧枚举回归、103透明泛型别名、105 CT枚举、最终树中存在的108/109测试、106四模块Native协议及102拥有字节Roam差分。性能候选只复核原始数据和身份，不在冷恢复重采。每项真实命令、首次失败、stdout/stderr和保留的原生材料进入verification附件。

交付脚本只核对现行35项必要门、十片非作者审核及既有原件，不放宽语言或产品要求。完整语言`NotSatisfied`、产品`NotRun`、B0 `Doing`与有限draft门分别报告；不同提交的编译与运行成绩不得拼成同一版本的四目标胜出。

## draft21 历史交付入口

本轮七件按`zil-rust-0.1.0-draft21-20261025`命名：`-source.zip`、`-history.bundle`、`-audit.zip`、`-verification.zip`、`-report.json`、`-report.md`、`-SHA256SUMS.txt`。生成、冷恢复和保存是否成功只读旁置机器报告与实际回执，本文不是运行证明。

当前主树沿“zil施工2”的Git与工具链继续，原097/098/099未提交源码逐字保留后提交，最初运行原件独立归档。历史bundle包含打包时所有可见refs及全部worktree HEAD的提交闭包，保留NoGo原候选、生产回退、作者和非作者审核身份；不重写原历史。当前audit包含本轮引用的原始档案和完整正式门。旧轮外置audit仍保留各自来源范围，不声称收集了所有历史未跟踪目录。

源码ZIP可独立构建；继续施工建议同时恢复完整bundle。正式受测提交、仅追加说明/证据后的包装HEAD及冷恢复身份分别记录。冷恢复重新核对全refs/提交集合、所有源文件与Git相同字节，从空target构建精简源码并执行099源/raw/wire/native及098字节差分；不重采图研究性能。验证附件保留实际脚本、命令和输出。

交付封装只核验既有35门和原始结果一致性，不新增或放宽语言/产品门；最终裁决仍为有限draft门、完整语言NotSatisfied及产品NotRun。


## draft19 历史交付入口

本轮七件按 `zil-rust-0.1.0-draft19-20261025` 命名：`-source.zip`、`-history.bundle`、`-audit.zip`、`-verification.zip`、`-report.json`、`-report.md`、`-SHA256SUMS.txt`。实际生成、冷恢复及保存结果只读旁置机器报告和真实回执，本文不是运行证明。

源码为schema16；history包含打包时所有可见refs和detached worktree HEAD的提交闭包。089的B/K/L/KL全部冻结对象、失败与384进程/1536请求保持schema15身份。研究NoGo不删除候选；语言090–092作者、独审、普通原生及Wine兼容原件分别归档。早期开发命令未记录运行时HEAD的部分保留实际原件和这一限制，不回填伪造身份。

保留旧33必要门并新增slice-native-abi；冷恢复从bundle新clone和新target构建，复核当前切片语言/机器边界、两目标四次portable执行。089冷恢复只执行固定B/KL的332请求/24原生功能与真实计数，全部四变体的原统计另行逐数复算，不重新计时或声称四变体重新执行。原展开preflight经逐成员核验后压缩保存，映射和所有字节均在审计附件。

上轮draft18七件通过其真实delivery-files.json和save-result.json继续引用，不重复包装全部旧audit。完整未跟踪历史运行不是Git承诺的一部分；本轮所用原始运行均由附件manifest列明。工具适配的所有失败和修复也在恢复附件中保存。以下章节保留历史范围。

## draft18 历史交付入口

本轮七件为 `zil-rust-0.1.0-draft18-20261025-source.zip`、同前缀的 `-history.bundle`、`-audit.zip`、`-verification.zip`、`-report.json`、`-report.md` 与 `-SHA256SUMS.txt`。实际完成、字节数、摘要和冷恢复只由旁置最终报告记录，本文不替代执行。

源码按schema15恢复；history包含打包时全部可见refs和detached worktree HEAD的提交闭包，包括原NoGo、返修和独审分支。原仓库不重写历史。audit保留084冻结schema14全部样本与原始包、085–088作者/独审/失败、Win64实际程序与截图，以及Wine/Xvfb原deb和环境恢复记录；可重建Cargo对象和完整Wine prefix不列作交付，环境包不是操作系统映像。

正式门保留旧32个必要检查并新增真实 `win64-platform-closure`。冷恢复从bundle新clone和空target重新构建，以新输出运行关键语言边界、原生Linux和Win64兼容层闭环；实际计数逐项读取当次收据。084的功能恢复与原采样分列，不在冷恢复重新计时。Wine不能改写为native Windows，缺环境不能当Pass。

以下draft17章节保留历史交付身份，不把其计数或规范改贴本轮。

## draft17 历史交付入口

本轮最终交付由七件组成：`zil-rust-0.1.0-draft17-20261025-source.zip`、同前缀的`-history.bundle`、`-audit.zip`、`-verification.zip`、`-report.json`、`-report.md`和`-SHA256SUMS.txt`。实际完成状态、字节数、SHA256和冷恢复观察以旁置机器报告及校验清单为准；此说明本身不代表已打包或已恢复通过。下面的通用三件打包命令仍适用于日常源码归档，不能替代本轮七件交付的独立恢复。

本轮history记录打包窗口内全部可见refs及detached worktree HEAD的提交闭包，先在独立mirror中命名detached HEAD，再制作bundle，不改原仓库refs。恢复核对全部refs、精确提交集合、fsck、非shallow且无alternates；完全不可达及仅存在于reflog的对象不在承诺范围内。

生产源码按schema14恢复；080性能证据保持原schema13的基线与候选身份，单独重建并核对功能，不把旧计时改标为14或再次采样。原083候选的真实失败、修复及严格SIGILL断言复审分别保存。旧draft16七文件通过其真实保存回执和哈希继续引用，不声称重新收集了所有历史未跟踪运行。

完整冷恢复使用新的target，分别记录119次本轮原生观察、80次既有组件原生行为，以及080的136研究请求和24次研究原生观察。不同运行与不同证据等级分列；任何实际失败都保留在原尝试目录。工具独审及实际命令收据在verification附件中，原始运行在audit附件中。

## 为什么拆分

旧交付把每次运行的bin、全部展开IR、冻结源码快照和包含它们的Git历史都放进主ZIP。draft02的22,781,405字节中，源码crate本身压缩后只有186,128字节；12,207份evidence和独立Git历史对象构成主要体积。这些原件仍有审计价值，但不需要成为每次构建或阅读源码的前置下载。

`.agent`保存当前决策，`.task`保存派工和审核关系，Git保存精确实现，evidence记录实际观察。保留证据的必要性不等于保留每次成功编译的全部中间文件在主源码包里。

## 三个产物

从干净main执行以下命令，输出放在项目之外或被忽略的out目录。已有同名交付拒绝覆盖。

```sh
python3 tools/package.py ../delivery/zil-source.zip
```

| 产物 | 内容 | 使用场景 |
|---|---|---|
| `zil-source.zip` | 完整源码、规范、工具、样例、任务、当前决策、紧凑结果/审核及关键失败小文本 | 日常阅读、构建、开发入口 |
| `zil-source-audit.zip` | 完整现有evidence，以及明确选择的新raw运行；每个文件有路径、大小、hash及执行位 | 核查旧观察，恢复原始失败与冻结回放材料 |
| `zil-source-history.bundle` | main及其可达原始实现、独立审核绑定与合入历史 | Codex继续原历史、worktree与handoff核验 |

每个附件有独立`.sha256`。源码内`PACKAGE.json`记录来源提交/脏状态、附件名称/hash、外置范围和实际体积统计；`SHA256SUMS`绑定源码包中的所有普通文件。审计包使用普通ZIP，不增加自定义内容存储系统。

正式历史bundle只从干净main创建，避免源码与历史错配。开发分树可显式传`--without-history`测试包装，manifest会写明历史未提供，不能由此声称handoff可通过。包工具不接受把已精简的交付目录再次包装成“完整原始审计”；重新交付应从恢复的源码checkout开始。

## 直接构建与核对

安装`rust-toolchain.toml`要求的Rust，以及当前后端要求的clang后，精简源码单独即可构建。无需审计ZIP或Git历史参与编译。

```sh
python3 tools/package_check.py
cargo build --workspace --offline --locked
cargo run --offline --locked -p zil-cli -- run examples/modules/main.zil --entry main
```

`package_check.py --companions 附件目录`还会检查旁置附件字节是否匹配manifest。它检查完整性，不重算旧测试，也不证明语言功能完成。修改源码后旧SHA清单应失败；不要使用`--write`把一个已经交付的manifest原地洗成新的通过记录。

## 恢复历史与原始证据

接续原施工历史时，使用`PACKAGE.json`命名的实际bundle。以下示例假定附件位于当前解压目录的上一级：

```sh
git clone --branch main ../zil-source-history.bundle zil-checkout
cd zil-checkout
node tools/handoff.mjs --check
```

无Git的精简源码包只支持源码构建；handoff仍应明确失败并要求恢复历史。旧历史仍包含当时已提交的大型evidence，拆包不会改写Git对象，也不会让旧bundle自动变小。已忽略的新raw运行只在审计附件里，不会因为clone了bundle而自动出现。

从精简源码目录检查并恢复审计附件：

```sh
python3 tools/evidence_archive.py verify ../zil-source-audit.zip
python3 tools/evidence_archive.py restore ../zil-source-audit.zip --destination .
python3 tools/package_check.py
```

当目标有`PACKAGE.json`时，restore会自动要求附件SHA与之匹配。恢复到其他空目录/checkout时，可额外传`--expected-sha256`与交付摘要绑定。没有外部预期hash的verify只检查ZIP内部一致性，不证明来源真实性。

restore先核验全部清单和已有目标，再写入。拒绝路径逃逸、非规范路径、重复/大小写冲突条目、软链、缺件、额外条目、内容篡改及覆盖已有不同字节；已有相同字节可以复用。恢复的审计清单仍受原源码manifest的hash约束，后续篡改或缺件会让package_check失败。磁盘故障可能留下部分新文件，命令会失败；再次运行可复用正确已写文件，不把半次恢复当完成。

恢复目标的所有既存祖先也必须是真实目录；祖先中的软链或可识别的junction会在读取目标元数据、创建目录之前拒绝。若系统常用路径本身包含软链，请检查后显式提供真实目录路径。此处是静态路径预检，不声称抵御其他进程在检查后并发置换目录；恢复目录应由当前操作者控制。

旧报告中的机器绝对路径只表示当时来源；当前文件由审计清单中的相对路径定位。旧的失败仍是旧失败，恢复不把它改为通过。历史回放还受原记录中列出的OS、动态库、工具链边界限制，ZIP不是密封的操作系统镜像。

## 控制后续增长

新领域runner使用`out/runs/<领域>/<RunId>/`，每次目录不可覆盖。主树只晋升必要的验收收据、独立审核、最小失败输入和修复前后小日志；成功bin与可重建IR保留在原始运行目录。旧runner的默认目录本轮未全部迁移，完整旧evidence仍封存，不能宣称已清理Git历史。

打包时明确选择本轮需要保留的新raw运行，可以重复此选项：

```sh
python3 tools/package.py ../delivery/zil-source.zip \
  --raw-run out/runs/roam/具体RunId \
  --raw-run out/runs/validation/具体RunId
```

工具不会自动扫整个out，也不会自动归档所有旧开发运行。manifest列出选择的raw根目录；未选择的运行不得被最终验收引用为已交付原件。所有原始evidence都在审计ZIP里；主ZIP只保留工单直接引用、当前汇总及直接子报告、切片顶层小收据/日志、审核、首次失败汇总及小反例，不递归带入所有case级result.json。

## 从当前Git树外置旧raw

集成者可以在最终测试和收据固定后先生成并核验完整审计包，按源码ZIP文件清单审阅需要外置的路径，再从当前树移除冗余raw并提交。不得改写已有Git历史；精确审核提交仍可恢复。包工具自身不会删除源文件。

清理提交后的正式包装使用`--audit-input 先前已核验的审计ZIP`。工具重新验证该archive，要求当前每个现存evidence及显式raw-run文件都在其中且字节相同，然后完整逐字复用；外置的原始失败、bin和IR不会丢。当前source/历史HEAD可以是清理后的新提交，audit内部来源仍保持原快照，两者分别记录。若此后变更或增加收据，复用明确失败；应在清理前重新准备包含全部新证据的完整快照，不能把历史结果偷偷覆盖。

需审阅的外置清单为审计`AUDIT-MANIFEST.json`中`evidence/`路径减去源码`SHA256SUMS`中的路径。只在核验归档后处理此差集，保留所有任务直接依赖和关键小反例。默认复制全部archive无需新的内容存储协议，也无需把历史bundle重新洗成无历史的仓库。

限定验证入口为`python3 tests/package.test.py`，使用真实小ZIP和Git仓库验证拆分、恢复和篡改拒绝。项目源码包另做解压后的真实构建、小程序运行和历史bundle恢复；第四轮结果见包外`DELIVERY-REPORT.json`，第三轮`evidence/impl/007/`仅作历史收据。这里的检查不替代完整语言验收。


## 第四轮审计范围与历史归档链

第四轮源码沿原 Git 历史续接，不重建或改写旧提交。本轮审计包包含当前精简 evidence 树，以及交付 manifest 明确选择的第四轮原始运行目录；它不会声称自动带回第三轮已经外置的全部 raw 文件。旧提交、独立审核对象和已提交过的证据仍可从完整 Git bundle 取得。

第三轮未纳入 Git 的原始运行仍由既有 `zil-rust-0.1.0-draft03-20261023-audit.zip` 保存：24,752,836 字节，SHA-256 `a2e945d48c8685a97f690793f762619f2e02dde727f866eb2cff624477b47a36`。第四轮继续保留此引用；审查旧运行时同时取对应历史审计包，不能把当前可选审计包当作所有历史原件的合并备份。新旧包的原始结果、首次失败与当时来源身份分别保留。

公开对话的54条文字与采集JSON另附小型设计来源归档，源码中只保留逐项审计、当前决策和原文身份。没有取得的聊天附件正文不在“已核对”范围内。

## 第五轮交付范围

第五轮主源码包含016共享捕获、017持续质量工具，以及018候选的文本补丁；018没有合入main，其生产提交不在main-only bundle的可达集合内。候选目录记录原始base和补丁摘要，可在历史base的独立worktree重建实验。不要把该补丁直接覆盖到已升级IR的当前主树，也不要把56项候选parser测试当成主线成绩。

本轮审计附件明确选择016作者/独审与共享源码oracle、017修订/独审和最终统一门的raw根；精确清单以PACKAGE.json为准。故障定位中有些目录只保存可核对的文本与摘要，并未复制每份历史可执行快照；相应独审记录明确边界，不将它们称为完整冻结回放包。新的正常bin、IR和构建缓存都不会进入主源码ZIP。

第四轮原始数据仍由既有 `zil-rust-0.1.0-draft04-20261023-audit.zip` 保存：13,342,301字节，SHA-256 `3749aaf3a275a0af3dac991223193df1f100fe2befce3862a53dc276f75cc417`。第五轮没有重新打包或改写该历史附件。当前包、历史包各自绑定来源，不把审计链描述为一个无限增长的合并包。

第五轮会对交付ZIP重新解压，验证所有源码/附件hash，从精简源重新离线构建并实际运行borrowlab、closurelab和numberlab；同时从本轮bundle恢复Git并执行冷启动handoff。其实际结果由包外 `DELIVERY-REPORT.json` 记录；源仓库内验收报告绑定受测提交，最终打包HEAD由PACKAGE.json绑定，避免自引用哈希循环。

## 第七轮主线交付

本轮主源码为 draft07，包含025–041已合入主线、schema/wire 4、多模块主应用及Rust行为对照。主包保留完整源码和小收据，不带工具链、Cargo target或运行二进制。质量工具仍为独立Rust workspace，首次准备依赖时按其锁文件fetch；生产编译器仅使用标准库与本项目路径依赖。

本轮审计范围明确选择最终统一门、两次失败的统一门、早期集成预跑及schema4主应用阶段产物。失败不会用后一次通过覆盖；未选择的日常作者raw目录不宣称已完整封存。具体选择由PACKAGE.json列明。每个精确候选的实现、反例源和独审绑定仍可从历史bundle恢复。

包外DELIVERY-REPORT.json记录实际打包来源、每个附件的hash、源码解压后的独立构建/主应用执行，以及bundle恢复的handoff结果。仓库内evidence/REPORT.md和final-summary.json绑定真实受测提交；最终源码HEAD允许只追加结果、当前文档和交付元数据，避免自引用哈希循环。

日常拿源码ZIP即可构建。Codex沿原工程历史继续时建议同时取history.bundle，以便验证提交和派工；只有追查原始执行材料时才需要audit.zip。候选语法、工具命名与下一阶段真实缺口保存在当前决策和工单，不要求重新阅读完整聊天。

## 第十一轮交付范围

draft11包含055真实TypedIR研究及056–058有限同步错误链，编译器schema/raw wire为8。源包仍可独立构建；继续开发应同时恢复history.bundle以取得精确候选和任务审核历史。正式门绑定受测提交，打包来源由PACKAGE.json记录，包外交付报告记录实际解包、冷构建、主应用运行及Git冷恢复；三者分别说明，避免自引用。

055正式19源/3192性能样本保留draft10/schema7及3d4375a身份，包括全部逐点预期关系、原始样本、统计和两次零计时失败/失格记录；schema8四模块19函数的完整功能shadow另存。主包保留必需小收据，完整原始TypedIR、事件关系、日志和仍存的机器产物进入audit附件；其实际raw根只以PACKAGE.json为准。当前恒等点切割候选没有建立优于块算法的完整成本收益，未进入生产安全核。

056作者suite已执行的65次原生行为有源码与真实测试日志；测试结束会清理临时机器文件，不能称原执行二进制全部留存。独立12次近邻probe保留了源、LLVM及机器产物。所有原件按其真实提交与运行目录封存，早先磁盘满/SIGBUS基础设施失败不作为语义拒绝或通过。旧轮次已外置的完整raw继续由对应历史审计附件负责，本包不是所有旧原件的合并备份。


## 第十二轮交付范围

draft12包含059联合图关系研究和060–062完整拥有package，编译器schema/wire为9。三组package_ui程序及独立Rust对照在源码包内；其临时测试产物会删除，另行保留的CLI阶段IR、原生程序及原始运行仅以audit清单为准。多模块接口哈希首跑panic及修复记录保留，不把失败归为通过。

源码、history.bundle、audit以及包外冷恢复报告分别绑定实际身份；旧draft11及以前的原始数据继续按对应历史审计附件保存。最新正式门只读final-summary，打包HEAD只读PACKAGE.json，冷恢复只读包外报告，不相互覆盖或自引用。


## 第十三轮交付范围

draft13包含063编译成本/联合关系研究及064–066具体环境公共helper，schema/wire10。helper_ui保留ZIL源码、独立Rust行为对照、公开接口与100次native行为输入；raw实际目录由PACKAGE.json列明。源码包、完整Git bundle、审计附件和冷恢复验证分别绑定其身份。063按其冻结schema9依赖和精确研究提交恢复，不拿当前schema10依赖重放旧数。

用户已明确授权将draft12七件交付及完整Git历史保存到当前登录账户，并持续授权后续交付保存。draft12七件保存有成功回执；draft13新成果在本轮完成冷恢复验收后按相同授权保存。历史审计附件仍各自承担旧轮原件范围，不将新包描述成所有旧raw的重复合并备份。

## draft14 七件交付

本轮源码含067–070，生产schema/wire11；067/068测量仍固定schema10输入。源码ZIP、完整Git历史bundle、完整本轮audit、包外冷恢复验证、delivery-report.json、SHA256SUMS及report.md分别提供。完整raw包必须包括067/068/070的portable档案、独审额外机器输出、主树正式门和首失败，不以compact receipt替代。

067的作者摘要曾把精确预算抄成345002；原始两份记录一直是545002，独审复跑一致。更正提交和旧d92均保留，原始档案与性能采样没有重写。冷恢复从独立目录重新构建当前编译器并运行保存环境应用；067单独从冻结schema10依赖恢复，不拿当前schema11重标旧数据。当前正式门、包HEAD、冷恢复各自绑定实际身份。

用户持续保存授权有效。draft12、draft13已完成的七件保留，本轮draft14在实际冷恢复及逐件成功回执后交付；不把本轮audit描述为所有更早raw的重复合并备份。

## draft15七件交付

本轮继续源码ZIP、完整all-ref Git bundle、原始审计ZIP、冷验证ZIP、机器验证报告、校验表与中文总报告七件。当前生产为schema12；071研究分别恢复400a082/schema10、067bf研究基线及04f覆盖，不拿生产schema12重新解释冻结结果。完整历史包含原作者/非作者审核、预算/源码修复和失败尝试。


冷验证从全新目录构建当前编译器，074三入口20固定输入在ZIL/Rust各O0/O2核对80次；071单独恢复并验证完整关系和确定预算，不重采历史计时。正式门、最终包HEAD、冷构建身份各自记录，原始审计逐member校验。实际完成状态只读本轮最终收据；这些交付约定不预先宣称未来命令Pass。持续保存授权有效。

## draft16七件交付

继续源码ZIP、完整Git历史bundle、原始审计ZIP、冷验证ZIP、机器报告JSON、SHA256SUMS和中文总报告七件。当前生产schema13；076研究交付树保留schema12规范身份，实际研究执行按400a082/schema10、067bf、07104f和0764172四层恢复，不能混用。正式受测提交、最终源码HEAD、研究执行及冷恢复身份分别记录。

全部可见refs及工作树HEAD经独立mirror保存；冷恢复fetch全部refs、核完整提交集合和对象ID，再运行fsck，无alternates。该声明不包含refs及工作树闭包之外的reflog-only/unreachable对象。本轮audit包含实际正式门、作者和非作者测试、首失败、精确工具链恢复及076完整3417成员归档；该归档内已保留原071包，不声称重复收集所有早期未跟踪raw。

外部六脚本经过非作者精确hash审核，随验证包交付。实际冷验证从源码ZIP构建14个workspace包，用新CLI构建owned_component三个入口；固定20输入×ZIL/Rust各O0/O2共80次，逐项核完整输出、退出码和环境视图布局。冻结076另从交付历史恢复53项测试、五个N/N−1预算、24对完整函数关系、120对raw关系及两组旧063反例；UnsupportedMultiplicity保留为未支持，不计安全接受。不重采性能。

正式门之后只允许文档、evidence、README、handoff和逐项列明的工单元数据变化；代码、配置、可执行研究和changes正文必须已冻结。每次失败用独立目录保留，不以降低计数或更换身份恢复成功。实际包外报告与七件校验表在成功冷恢复后产生，保存回执再由真实上传动作记录，不能预填。


## draft20七件交付

本轮继续源码ZIP、全部可见refs及detached worktree HEAD提交闭包的完整Git bundle、audit ZIP、verification ZIP、机器报告JSON、中文报告及SHA256SUMS七件；实际保存回执另记。公开设计讨论仅保留清洗后的用户/助手文字及原Boundary Lab包，不交付浏览器bootstrap或个人上下文搜索记录。

093研究固定schema16/38b9169，H生产NoGo且完整补丁和55521448历史保留；其完整raw归档包含8690载荷文件，64,244,950字节，SHA256 a5e66fda43168700baa3a0a08fe72bdf1dc290b26f191d3e364ae0c032b5ed60。当前生产schema17，不重标旧计时；冷恢复仅复算原样本与身份控制，不重新取样。

094–096的作者、非作者和正式测试分别保留首次失败及修复。冷恢复应从实际源码ZIP解压到新目录构建，核动态字节消费者与Rust的行为及便携Linux/Wine程序；完整Git另恢复全部refs/提交集合并fsck。具体执行次数与结论只以本轮包外实际报告为准，不把本段约定当作完成声明。旧draft19及更早raw仍由各自历史附件负责，本包不假称重复收集所有旧未跟踪运行。
