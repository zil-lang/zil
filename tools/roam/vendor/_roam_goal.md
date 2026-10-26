# Roam 实施目标

日期：2026-10-18。工作区：`D:\res\cc\zit`。

状态：通用协议与宿主已落地，本轮复审修正独立重放、PW 探索和执行边界；第二语言内核与公开发布仍待实施。现有测试入口已迁出 zst，但领域场景转换不等于全部完成，边界见下文。

## 目标

根目录 roam 为内核、UI、editor、BLR 提供动作案例、性质判定、探索、失败保存、重放及独立缩减。长期工作流为：冻结构建 → 独立探索 → 原始失败可靠保存 → 重置并继续；复现、缩减、修复在独立工作流进行。门禁有界并对非通过明确失败退出。

PW 作为可选执行与观察插件，固定路径与生成路径共享模型、动作和 oracle。轻量状态搜索保持进程内入口；无需为了旧 API 兼容保留隐式转发或重复生命周期。未来原生 roam 通过同一 JSON/JSONL 语义契约对接，不把 JS 内部对象和微任务轮数当公共规范。

## 不变量

- 纯 `core.ts` 不依赖 Zit、DOM、Node、PW、editor 或 BLR；默认 `roam.ts` 是 Node 宿主入口；产品内核与 UI 不反向依赖 roam。
- 私有模型、fixture、案例留领域目录；公开包采用白名单，当前仍 private，不公开发布。
- 不降低原断言、修改 expected、跳过既有测试或删减测试档位换取通过；不修改产品逻辑。
- 原始轨迹不可被缩减、重试或修复覆盖。seed 只作溯源，重放使用实际 JSON 动作。
- 区分性质失败、设施错误、无效案例、不支持及预算耗尽；执行器合作式预算不冒充进程抢占。
- 失败落盘成功后才继续。生成或存储设施失效应明确停止，不能无限空转。
- 固定依赖闭包不受源码热更新影响。重放默认使用原件快照，显式选择新实现才重新构建。
- 静态装配、本机文件存储、直接子进程监督足够；不引入插件注册平台、分布式调度或第二套 PW 队列。

## 目录与责任

- `core.ts`：严格 JSON 协议、领域接口、单次执行和数据生成器；`shrink.ts`：同一性质的严格候选判定，单向依赖核心。
- `search.ts`：任意 JS 状态/动作的有界 BFS、随机搜索、批量试验。默认入口不再转发。
- `check.ts`：断言和差异；`clock.ts`：显式宿主时钟和现有测试使用的串行时钟；基准 `bm` 位于 `zit/__`。
- `roam.ts`：Roam 类的探索、门禁、重放及缩减工作流；`store.ts`：文件证据与租约；`worker.ts`：JSONL 监督；`snapshot.ts`：冻结构建；`cli.ts`：参数及报告适配。
- `plugins/node.ts`、`plugins/browser.ts`、`plugins/pw.ts`：按需装配的测试宿主、浏览器报告与 PW 执行。
- `zit/test/roam.ts`：Zit 专属 settle/audit；领域 driver 与模型留在对应目录。旧 `zit/zst.ts` 已退出。

## 协议与执行验收

- 无损 JSON 拒绝 undefined、特殊对象、访问器、循环、稀疏数组、NaN/Infinity/负零；特殊值由领域显式编码。
- contract 带语义版本；未实现的非空 schedule 显式 unsupported。交错使用可保存的领域交付动作。
- 初始化部分资源及时注册清理；检查初始化态、每步及终态；清理不抹除主失败。
- 异步初始化、前置条件和宿主 ack 后重查预算；真实动作完成即更新动作计数，不能被模型失败改写为协议错误。
- 性质失败数据先冻结并校验可保存性；不可保存的诊断明确归设施错误。
- 缩减只接受同一 propertyId 的有效失败，原件不变；预算包含最后一次重放，不宣称全局最短路径。

## 独立探索与重放验收

- 领域初始化、生成、执行均受子进程监督。动作持久记录后 ack，stdout 只用 JSONL，诊断走 stderr。
- 探索遇到 crash、hang、性质失败先保存具体路径，资源可靠收口后执行新实例；生成、保存或进程树清理设施失效则明确退出。
- 删除原领域源码后，replay/reduce 仍使用原快照；哈希变化拒绝执行；新源码回归必须显式指定 driver/build。
- replay 重复确认，区分稳定、间歇、不复现和其他失败；缩减另存。角色租约防重复领取及过期写入。
- 文件 fsync/同卷 rename 保证进程中断恢复，不宣称断电事务；锁仅按 owner/token 显式离线恢复，不按时间偷锁。
- 监督根为直接子进程，领域不得脱树或派生独立后台任务。硬停尝试回收自有树；清理失败保留主错并保存原件后停止。PW 正常/内部超时主动关闭浏览器；Windows 已退出根后的孤儿不可追索，不宣称完整恢复。

## PW 验收

- Page/context 每例隔离；冻结 fixture，无开发服务器/HMR；真实输入与 editor-api 分别标记。
- 首次执行即记录 trace，动作到释放/trace/context 收口的错误均保留；最终成功才删除 trace。
- PW 使用同一 Roam/CLI 的 explore/replay/reduce；只新增可选快照装配，不再建设独立 runner、队列或缩减器。
- 固定和生成场景共享驱动及 oracle；至少验证负控失败保存、随后继续以及独立重放。
- UI 验证 raw/sig、开关/禁用、焦点、表单值与提交、自动补全和异步请求交付。editor 验证 API 正文/撤销及 DOM 同步，不宣称 IME 或独立 Unicode 判定。

## 现有测试迁移状态

- 原 102 个本地消费者已按职责迁移 import/export，产品逻辑未改；本轮将进程内搜索消费者改为显式 search 入口。
- 内核值/异步 intent、容器数组/Map/对象与派生值、UI 组合、editor API、BLR 顺序交付已接持久化案例。
- 其余 FSM、快速 DOM 与 md/latex/flow/press 等领域仍保留原判定和进程内搜索。BLR 复杂并发/slim/fresh/codec 仍由原测试覆盖，尚未逐项转成协议案例。
- 保留这些测试是覆盖要求；不把“入口迁移”表述成“所有场景已获得持续探索和严格重放”。完整替换须逐领域保留动作与 oracle，不能通过泛化桥接静默丢失闭包、undefined 或性质身份。
- 浏览器报告仍服务 demo；备份/现有路径不删除，不将页面内 evaluate 等同真实用户输入。

## 验证命令

```powershell
bun test ./roam/test ./zit/test/roam.integration.test.ts ./blr/roam.integration.test.ts ./ed/roam.driver.test.ts
node node_modules/typescript/bin/tsc -p roam/tsconfig.json
bun roam/test/foreign.conformance.ts
$env:ROAM_PW_CHANNEL = 'chrome'
pnpm exec playwright test --config roam/test/fixtures/pw.config.ts
pnpm exec playwright test --config roam/test/fixtures/editor.config.ts
```

独立探索、快照与重放的命令见 README。默认预算有界，不因导入自动长跑。本机默认配套 headless shell 未安装，验证显式使用现有 Chrome，不下载或修改全局浏览器配置。

## 实施证据与实际边界

首轮：43 项机制/领域测试通过，UI PW 6 项、editor PW 1 项通过；独立包 18 文件打包与无 node_modules 核心导入通过；Python 与 TS counter 正负案例协议一致，非第二语言内核等价验证。

既有测试基线：基础/异步 97 项通过；扩展 392 项中 zit-clr-6、zit-sch-13 两项在开工快照同样失败；另 67 项中 press-render-2、press-render-5、press-gate-1 在开工快照同样失败。完整 UI/editor 类型检查原已非全绿，首轮未新增诊断。未为这些失败修改产品或裁判。

本轮复审结果见下方验收记录。当前 PW 快照依赖本机 Node/PW 安装；跨语言传递的是 JSON 行为契约，不宣称本机快照等于跨机容器。运行证据位于 test-results/roam-*，不进入公开包；按功能点提交，保留原暂存/未暂存修改。


### 本轮复审验收

- `bun run test-roam`：79 pass、0 fail、406 断言（含 host 25 项）；严格 ES2022 核心检查通过，PW 插件/测试定点类型检查通过。
- 相关既有门禁：base/async/rx 与 UI spec/layer 共 167 pass、0 fail；原判定与旧 pick 权重保留。
- UI PW：6 项固定/生成与负控通过；新增独立 host 闭环定点另通过，合计 7 项均有通过证据。闭环含两次负控失败后继续、不带 driver/runtime 重放稳定 2/2、原件不变、内部期限退出与首次 trace。
- editor PW 固定路径 1 pass；最终模型新 seed7/8 各 40 步通过，原 seed8 40 步通过；原 seed7 在 step=23（从 0 计）按修正后的前置条件 invalid，旧原件未改。误报来自快照恢复光标的模型桥，现用独立命令增量，产品未改。
- 假时钟原子安装/描述符恢复、falsy 异常、严格失败载荷、动作计数、预算边界、缩减身份、冻结动态 require、运行时漂移均有定点回归。
- 原件删除源文件后仍可使用冻结快照重放/缩减；篡改快照拒绝执行。崩溃原件保留，资源清理设施失败则保存原始 Timeout 并追加 TreeCleanupError 后停止。
- Windows 受管理沙箱拒绝 taskkill /T；已验证拒绝后的停止、落盘与管道收口。正向整树回收未验证，提权验证等待未返回后取消，没有把它计为通过。测试派生的 heartbeat 后代均以 stop 文件收口并确认记录 PID 退出。
- 运行日志位于 `test-results/roam-audit/` 及 PW 配置输出目录；CLI 编辑器证据在 `test-results/roam-editor-host/`。本轮不把本机快照、协议符合性或入口迁移表述为所有领域完成迁移。

- 最终独立包 19 文件、38.54KB 压缩包；白名单不含领域模型/测试，解包到无 node_modules 的临时目录后 Bun 纯入口及 Node ESM 构建导入通过。
- 最终 Python/TS counter 的 pass/fail/propertyId 保持一致。

### 架构收敛验收（2026-10-18）

协议与单次执行合入 core.ts；缩减显式单向依赖核心。Roam 持配置，操作状态属于每次调用；CLI 仅参数与报告适配。存储、进程监督、快照按资源职责拆分，旧 protocol/execute 入口及混合 host 职责退出。PW 生命周期移为普通 TypeScript，生成入口仅装配。领域修改只涉及必要引用，不改产品和判定。

- 完整 Roam 门禁：83 pass、0 fail、427 断言（12 文件）；包含新增 Roam 程序化门禁、存证后独立重放/缩减和快照篡改拒绝。
- UI PW：7 pass；editor PW：1 pass。实际输入、负控、失败后继续、冻结重放及浏览器 deadline 路径通过。
- 严格核心类型检查及宿主/PW 定点类型检查通过；Python/TS 协议符合性通过。
- 独立包：21 文件、40.73KB 压缩；解包到无 node_modules 目录后核心及 Roam 导入通过。尚未公开发布。
- 缩减期间若监督器报告进程树清理失败，保存证据后停止后续尝试，最终显式报错；不将缩减器捕获异常误认为可继续。
- 既有 Windows 整树回收正向验证、全领域迁移及第二语言内核等未完成边界沿用上文；本轮是架构重构，不据此扩大功能完成声明。

决策见 .agent/architecture.md；日志在 test-results/roam-audit/refactor-*。

提交状态：本轮实现与验收完成，Git 对 .git/objects 写入返回 insufficient permission；Windows ACL 存在继承 Deny，当前会话禁止提权请求，因此尚未创建提交。功能补丁保存于 test-results/roam-audit/01-roam-architecture.patch 与 02-roam-pw.patch。

### 统一命名与接入（2026-10-18）

宿主类统一为 Roam，选项为 RoamOptions，从默认 roam 入口导入；核心保持纯。CLI、领域引用、测试、发布清单和文档使用当前入口，不保留 Runner 或 executeCase 别名。search/check/clock 是现有独立能力，不是旧模块转发；接入新入口不等于全部领域都已转换为持久化 JSON 场景。

改名后完整门禁 83 pass、0 fail、427 断言；核心与宿主严格类型及 Python/TS 协议验证通过。场景提升规则见 .agent/regressions.md，不为此新增注册平台或自动晋升工具。Git 对 .git/objects 的写入本轮再次被拒绝，尚未创建提交。

统一命名后真实浏览器复验：UI 7 pass，editor 1 pass；独立包 21 文件，解包后 Roam 与核心导入通过，Runner/executeCase 别名不存在。功能补丁按序应用到原基线后与最终工作树逐字一致（文本换行归一），提交仍受上述权限限制。

### 入口、搜索与共享模块收敛（2026-10-18）

默认 `roam.ts` 导出 Roam，纯协议和单次执行位于 `core.ts`；不保留 host 转发。Roam 方法返回报告，CLI 独占退出码映射。`search.ts` 的随机入口为 `fuzz/fuzzFsm`，默认首个失败停止、默认不缩减，可用 `maxFailures` 和 `minimize` 显式选择。性能基准 `bm` 移至 `zit/__`；UI 改用 `zit/shared` 同一模块子路径。既有领域引用已同步；发布前仍需审定许可证及公开面。

本轮验证：Roam 全量 84 pass、0 fail（428 断言），Zit 基础 11 pass，UI 单测 16 pass，UI PW 7 pass，editor PW 1 pass，Python/TS JSONL 协议正负例一致，Roam 类型检查通过。`test-zit` 全量另有 `zit-sch-13` 失败；UI 全量类型检查仍有既有 chart/flow/md/ui/ssr 等诊断，本轮未以修改产品逻辑掩盖。Git 对 `.git/objects` 写入仍报告 insufficient permission，未能按功能点创建提交；工作树改动及原有暂存保持原状。

2026-10-19 提交恢复：普通 workspace-write 沙箱仍拒绝 Git 对象写入；通过当前自动审批的沙箱外执行验证写入成功，按基准归属、Roam 架构、UI 共享入口三个功能点提交。使用独立暂存区排除既有内核和其他业务改动，并保留原有暂存状态；内核优化仍仅讨论。
