# Roam

Roam 提供状态与序列探索、严格路径重放、失败缩减，以及可选的浏览器执行插件。`core.ts` 不依赖 Zit、DOM、Node 或 PW；默认 `roam.ts` 是 Node 宿主入口。领域模型由调用方装配。

目前使用 TypeScript 源码入口，Bun 可直接运行。包名暂定 `@zit/roam`，保持 `private: true`；许可证和发布授权尚未确定，不执行公开发布。原 `zit/zst` 活动入口已退出；性能测量 `bm` 位于 `zit/__`。

## 入口

- `roam.ts`：Node 宿主的 `Roam` 类，持配置和存储；方法直接返回结构化报告。
- `core.ts`：无宿主依赖的 JSON 协议、领域接口、`execute`、`property` 和数据生成器。
- `shrink.ts`：独立路径缩减，只依赖核心，不由默认入口转导出。
- `search.ts`：进程内 `bfs/fuzz/fuzzFsm/trials`，与持久化执行入口分开。
- `check.ts`：断言、差异和旧测试计数。新会话执行器不依赖这些共享计数。
- `clock.ts`：显式拥有的 `createClock(host)`；`clk/$clk` 供串行测试共享使用，不用于并行会话。
- `store.ts`：不可变原件、派生结果、领取与续租。
- `worker.ts`：JSONL 子进程监督与服务端通信。
- `snapshot.ts`：冻结领域执行入口。
- `cli.ts`：参数和报告适配，复用 Roam。
- `plugins/node.ts`、`plugins/browser.ts`、`plugins/pw.ts`：按需导入，不由核心自动加载。
- `zit/test/roam.ts`：本仓 Zit 的 `settle/audit`，不属于此包。

默认入口是 Roam；纯执行显式导入 core，进程内搜索显式导入 search。`search` 支持原有任意 JS 状态及动作，用于轻量有界检查；它的错误字符串、种子和贪心缩减不提供严格持久化契约。需要长期探索的领域必须导出 JSON 案例、具名性质和驱动，再接入 Roam。

## 使用边界

轻量测试只需要 `execute(spec, driver, case)`。需要隔离执行、持续探索和保存失败时使用 Roam；领域仍导出 `spec / driver / generate`，不继承框架类。

```ts
import {Roam} from './roam';

const roam = new Roam({
  root: 'test-results/roam',
  target: {driver: 'zit/test/roam.driver.ts', build: 'local-build'},
  timeoutMs: 10_000,
});
const result = await roam.explore({seed: 1, cases: 100, steps: 48});
// 另一个进程可独立领取证据，不需要原领域源文件。
const replay = await new Roam({root: 'test-results/roam'}).replay({repeat: 3});
```

Roam 仅持有配置和存储，各次调用独立持有预算、计数与租约。CLI 根据报告计算退出码；程序化调用不混入进程状态。核心与缩减使用函数；存储和宿主使用对象，不提供领域继承层级。PW 工作流是普通 TypeScript，冻结时的生成入口只装配依赖和配置；UI/editor 的 fixture 构建仍由各领域负责。

## 定义与重放

驱动模块导出 `spec`、`driver` 和可选的 `generate(seed, steps)`。模型可以很小，也可只描述需要检查的性质。真实实现与模型分别执行，`property(id, condition)` 失败后产生稳定性质编号。

```ts
import {execute, property} from './core';

const spec = {
  contract: 'counter/1', init: () => 0,
  step: (m, a) => m + a.value,
  check: (m, observed) => property('counter.value', m === observed),
};
const driver = {
  init: () => ({value: 0}),
  step: (r, a) => {r.value += a.value;},
  observe: r => r.value,
};
const result = await execute(spec, driver, {
  format: 'roam/1', contract: 'counter/1', steps: [{op: 'add', value: 2}],
});
```

案例只含无损 JSON：有限数值（拒绝负零）、字符串、布尔、null、普通对象和密集数组。undefined、BigInt、特殊对象、访问器、循环和稀疏数组显式拒绝。扩展编码尚未实现。依赖资源使用逻辑 ID；不保存闭包和对象地址。

`contract` 包含语义版本，`requires` 声明驱动能力。当前非空 `schedule` 返回 unsupported；需要交错时在 `steps` 中定义明确的任务交付动作，由领域驱动解释，禁止把未消费的调度数据报告成重放成功。

结果区分 pass/fail/invalid/error/unsupported/incomplete。初始化部分资源用 `context.defer` 及时注册；主失败与清理错误同时保留。`budgetMs` 是合作式检查，不能抢占一个不返回的 Promise；硬超时使用子进程宿主。

## 本机工作流

在仓库根目录运行：

```powershell
bun roam/cli.ts gate --driver zit/test/roam.driver.ts --build local-check --cases 20 --steps 40 --budget 30000 --root test-results/roam
bun roam/cli.ts explore --driver zit/test/roam.driver.ts --build local-check --cases 1000 --steps 100 --budget 600000 --root test-results/roam
bun roam/cli.ts replay --root test-results/roam --repeat 3
bun roam/cli.ts reduce --root test-results/roam
bun roam/cli.ts soak --driver zit/test/roam.driver.ts --build local-check --steps 10000 --budget 600000 --root test-results/roam
```

gate 有限预算，非通过结果或预算未满足时退出失败。explore 保存失败并继续独立案例；设施无法生成或保存证据时停止并明确报错。soak 是一条长会话，区别于很多条短会话。所有默认值都有界；长期运行由调用者安排，进程不会因导入模块自动启动。

CLI 在执行前用 Bun 将驱动与依赖闭包冻结为有 hash 的单文件。模块初始化、生成、案例执行都在受监督子进程进行。`--build` 是溯源标识，实际冻结文件 hash 另存；未解析动态模块加载会被拒绝。低层 `supervise` 接收任意可执行进程，调用者自行保证构建不可变。

新失败先保存原件，再重置实例继续；确认和缩减可独立启动。replay/reduce 默认校验并使用原件对应的冻结快照，无需原源码；显式 `--driver ... --build ...` 才构建修复后的实现进行回归。也可用 `--snapshot HASH` 选择同一证据根下的已有快照执行 gate/explore。`--runtime node|bun` 指定直接执行器，保存实际路径与版本；原件重放默认复用并验证运行时版本，版本漂移明确拒绝；显式选择新的 `--runtime` 或 `--driver` 才表示在新环境回归。不传 `--id/--case` 时 replay/reduce 领取相应角色队列，租约到期可被接续，完成记录不覆盖原始案例。固定案例用 `--case path.json`；指定已保存证据用 `--id ID`。缩减另存 `reduced-*.json`。

原件目录含 case/failure/environment/ready；受监督运行另附逐动作确认记录、stderr 和中断现场。逻辑类别聚类，原件仍各自保留；不自动删除历史失败。`--capacity` 限制证据目录容量，达到限制显式报错，重复样本的保留策略由调用方决定。

文件写入 fsync 后同卷 rename，支持进程中断后的显式恢复，不承诺断电事务。锁恢复必须先停止全部写入者：`recover-capacity` / `recover-mutex` 查询 token，再带 token 与 `--stopped yes` 恢复；仍活跃的 owner 会被拒绝。`FileStore.pending/recoverPending` 用于核查已死亡生产者留下的半成品。

监督根为本次直接子进程，驱动不得派生脱离原树的后台任务；PW 浏览器作为受管理资源，由内部期限和 finally 收口。硬超时等强制停止只回收该自有进程树：Windows 使用 taskkill /T，POSIX 使用独立进程组。若权限/设施使树清理失败，保留原始超时等原因并追加 TreeCleanupError，关闭直接进程及管道、保存原件后停止 CLI，不能假称已清理而继续。Windows 中外部 worker 已自行退出且后代脱树时无法追索孤儿，仍明确禁止此行为。

本机受管理沙箱拒绝 taskkill /T，已验证失败保存与停止分支；正常权限下的正向树回收尚未验证，提权验证等待未返回后已取消。PW 内部期限关闭浏览器的实际路径已验证，不能与宿主硬杀后的完整回收混为一谈。标准输出保留给 JSONL 协议，诊断使用 stderr。时间预算、输出预算、停止原因和 heartbeat 都会记录。

## 浏览器

`executePw(spec, driver, case, options)` 接收独立的新 Page/context，明确冻结 build、driverVersion 和 tracePath；执行开始即录 trace，最终确认成功后丢弃，失败则保留。插件监听 pageerror，保留释放阶段错误及原始性质失败。console.error 作为诊断记录，业务错误规则由领域性质决定。

固定和生成路径共享语义动作；locator/keyboard 驱动标记真实输入，API 桥使用单独 profile。UI fixture 在测试开始构建为内存静态 bundle，经 route 提供，避免共享 HMR。页面内快速驱动及浏览器报告仍在 `plugins/browser.ts` 保留，服务现有测试和 demo。

```powershell
# 已安装默认 PW 浏览器时无需设置；本机现有 Chrome 可显式选择。
$env:ROAM_PW_CHANNEL = 'chrome'
pnpm exec playwright test --config roam/test/fixtures/pw.config.ts
```

独立探索先准备冻结快照，再调用同一 CLI：

```powershell
$env:ROAM_PW_CHANNEL = 'chrome'
$target = (pnpm exec vite-node --script ui/roam.runtime.ts test-results/roam-ui | Select-Object -Last 1) | ConvertFrom-Json
bun roam/cli.ts explore --snapshot $target.snapshot --runtime node --root $target.root --timeout $target.minimumHostTimeoutMs --steps 48 --cases 1000 --budget 600000
bun roam/cli.ts replay --root $target.root --repeat 3
bun roam/cli.ts reduce --root $target.root --cases 100
# editor 使用 ed/roam.runtime.ts 和独立证据目录，后续命令相同。
```

`freezePw` 将领域驱动、fixture 与 roam 依赖冻结在单个快照中；Playwright 作为显式外部宿主，检查其入口/manifest 身份，浏览器版本随运行记录。这不承诺整个已安装 PW 包及浏览器二进制均已归档。当前 PW 快照绑定本机宿主安装路径；JSON 案例可跨机/跨语言交给匹配契约的驱动，快照本身不等于可移植运行环境。准备阶段使用 Node/vite-node；本机 Windows Bun 读取 pnpm PW/Vite 依赖存在 EPERM，实际浏览器 worker 直接使用 Node。内部期限先关闭浏览器，外层监督 timeout 应采用准备结果给出的最小值。

失败 trace/report 附件保存在证据根的 `.pw-attempts`，原件使用相对根的引用；迁移证据应保留整个 root（包含 `.snapshots` 与附件），单拷 UUID 原件目录不会包含完整调试资产。

这个配置不启动开发服务器。UI 组合试点位于 `ui/roam.pw.ts`，测试产物含实际路径、结果、冻结资源和负控 trace。Web 平台仍不是完全确定性模拟，低概率失败应重复确认并保留首次证据。

## 跨运行时与验证

JSONL worker 先发送 `hello/roam-worker/1` 与能力，接收案例；每步发送 before 并等待父进程持久化 ack，再执行并发送 after，最终发送结果。对象键顺序不影响协议相等性。外部语言可直接使用 `supervise`，不要求装载 TS。

```powershell
bun test ./roam/test ./zit/test/roam.integration.test.ts ./blr/roam.integration.test.ts ./ed/roam.driver.test.ts
node node_modules/typescript/bin/tsc -p roam/tsconfig.json
bun roam/test/foreign.conformance.ts
```

最后一个命令需要 Python，验证共享 counter 案例的通过及具名失败与 TS 一致；它是跨运行时协议符合性验证，不是新语言内核等价验证。`corpus/signals.json` 可用本仓 Zit 驱动重放。私有 BLR/editor 模型和案例不进入公开包。

## 领域迁移边界

- 内核：值与异步 intent 驱动，以及 `zit/test/rx.roam.driver.ts` 的数组/Map/对象和派生值 oracle，已支持具体动作保存、监督执行和独立重放。容器的 JS `undefined` 使用明确领域标记，不能由 JSON 静默丢弃。
- UI：Layer、Form、Autocomplete 组合共享真实 locator/keyboard 驱动；固定与生成路径共用模型。
- editor：明确标记 editor-api，保留现有正文/撤销及 DOM 同步判定；不冒充原生输入或独立 Unicode oracle。
- BLR：顺序编辑与逆序/重复交付已接协议；原有复杂并发、slim、fresh、codec 等检查继续运行。

其余 FSM、快速 DOM 与领域长跑仍使用 `search` 和既有判定，修改入口不等于具备持久化轨迹。没有删减这些测试，也不宣称全部场景已转换。浏览器报告仍可供 demo 使用，不能作为独立探索器。

独立包使用文件白名单，保留 private，测试打包可用 `bun pm pack --destination <临时目录>`。完整实施与证据见 [_roam_goal.md](./_roam_goal.md)。

## 将失败提升为场景钉

原始失败保留在证据目录，探索继续；修复流程独立重放、缩减并归因。确认产品缺陷后，把最小解释路径写进所属领域的现有测试：短路径内联，长路径用领域 corpus 的 JSON。固定钉针对当前实现断言正确契约，修复前失败、修复后通过，不依赖 seed 重新发现。原快照用于历史复现，不能作为修复后门禁的执行目标。

对于通用原因，补稳定性质和少量边界变体，再交给生成器探索更多组合。产品、模型、生成器、设施问题分别进入各自测试；PW 交互问题保留真实输入。原件不自动晋升为门禁，也不批量复制进源码。完整规则见 [.agent/regressions.md](.agent/regressions.md)。

## 包边界

`@zit/roam` 当前仍为私有包；发布前须确定许可证与发布授权。包内的 `core` 不依赖 Node、DOM、PW 或 Zit，`roam` 宿主依赖 Node，PW 按需导入。进程内 `fuzz` 默认首个失败即停，`maxFailures` 可扩大收集数，`minimize: true` 才执行贪心缩减；其非 JSON 状态不承担跨语言重放。

UI 对共享基础能力使用 Zit 的 `zit/shared` 子路径，同一包实例与 `zit/__` 保持相同模块身份。当前该入口仍导出完整 `__`；将来独立发布 UI 时，先清点实际使用的符号再收窄公开面，避免复制一份可变共享状态。
