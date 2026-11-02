# Type/Syntax 待冻结消费者源码套

这些是**拟议合同的源码与未来预期**，没有解析、编译、执行或全量测试。`EXPECTED.json` 的 observed 全为 null。不是现有语言通过证据，也未改动原 `sources/` 探针。原 `#(i32)` 和 `#emit #(1)` 两条字节原样复制。

全部入口位于同一 `project/` 目录，选择一个 `*_main.zil` 作为 root 即可共享该目录下的 generators/common/providers 模块；其他入口不导入，不要求同时含多个 main。消费者后续冻结时应连同 SOURCE-MANIFEST、provider lock、API合同和目标 profile 一起记录，不只抄 main 文件。

| 入口 | 未来 stdout / exit | 必须证明的实际行为 |
| --- | --- | --- |
| 00_frozen_type_value | 空 / 0 | 原 `#(i32)` 接真实 Type 值 |
| 01_type_values | 空 / 0 | 跨模块 Type 传返、数组构造、pub字段反射、生成getter及target布局查询 |
| 02_four_categories | KKZ42 / 0 | Expr/Type/StmtList/ItemList；纯CT循环拼接；同一Syntax片段两次插入得到独立tmp，外部tmp仍Z |
| 03_providers_hygiene | AAZ42 / 0 | 五个跨模块provider，其中两者同为Expr；括号种类不预设输出类别；next只执行一次；库support不被调用点整数遮蔽；新tmp不捕获调用者；输入新声明名可在调用者使用 |
| 04_generated_ownership | BAAP / 0 | 生成函数正常路径BA、早退路径A、未调用函数不创建资源，生成Package最终清理P |
| 05_generated_allocation | F / 0 | 一份ItemList同时生成新的WorkItem类型、make和check函数；Ticket/ByteBuffer成功转交与清理 |
| 05b_generated_early_error | F / 1 | 构造Ticket后立即错误退出，未调用reserve，不产生分配请求 |
| 06_generated_impl | T / 0 | imported owner模块调用跨模块CT Type函数，发射普通Drop impl；无当前impl查询反向影响生成 |

05在强制第一次 bytes_reserve 失败时预期 F / 1、退出live allocations为0；正常预期 F / 0、live0。**实际分配点数量尚未测量**，不可提前写已测次数。工作项数据的私有manages字段只在同一生成ItemList内的check函数读取，入口不跨模块偷读管理字段。

01的数值布局oracle只针对x86_64 LLVM host、i32 size4/alignment4（LP64或LLP64）；其他目标要另冻结其target/ABI预期，不能沿用宿主布局。生成器 `meta.layout` 查询与实际目标绑定，不是读取host usize。

`neg_*` 每条单独作为入口，具体拒绝义务见EXPECTED。类别/CT-only、private字段、名义身份、重复give、错误member owner等必须命中真实理由，不能把任意较早Unsupported记作安全拒绝。尤其 Hidden **确实存在private x**；跨模块getter失败应是可见性，不能用“不存在x”代替。layout自环应给依赖环/阶段诊断，不能无限重试后ResourceLimit。未知provider export须保留原tokens供preview。

## 普通ZIL语法已做只读对照

- `parser/items.rs::aggregate/field_after_prefix` 已支持普通 `pub x:i32`，仅package可写manages，且 `pub manages` 明确非法。本套管理字段均不加pub。
- import采用现有点路径和as别名；`syntax use … as …`显式别名和三种token tree定界已有parser结构。
- Ticket与Drop写法照现 `examples/mainline/impl174/model.zil`：`pub resource Ticket{pub code:i32}`、`impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}`。
- Package绑定管理字段显式 `give ticket`/`give data`；不把普通类型名或Desc升级拥有值。
- `void!i32`入口错误退出1来自现LLVM entry wrapper的已存在约定（只读代码核对，未运行本套）。

新语法/API仅为已推荐quote/$/#splice、CT-only Type/Syntax/Ident/TokenTree/Text及CT库绑定；普通函数、数组、import、struct/package/resource/Drop/error union均保持普通语法。所有quote资源语句都是未执行的语法数据，不能在CT VM创建运行期Ticket或调用putchar。

## 仍待公共冻结的最小API和锁

06的Drop实现由资源owner模块common.tracked中的#emit产生；impl_generators不导入tracked，不形成模块环，也不尝试为别的模块类型额外取得impl权限。生成的impl与同位置手写Drop经过相同规则。

`CT_API.suggested.json`列出实际源码所需std.meta/std.syntax签名。它们是**编译器认证的闭合CT库符号**，不是任意同名文件取得intrinsic权限。std模块的正式装载/分发归driver/公共合同；本套没有伪造可执行实现。Text是CT有界文本，不借运行期String/ByteBuffer权限。

`provider-lock.suggested.json`已记录五份provider源码真实SHA256；CTIR、grammar、schema正式hash尚不存在，保留null/待冻结标记，不能拿建议清单当production lock。每导出固定一个category、原始TokenTree输入、支持delimiter和明确entry symbol；同类别两provider无导入顺序优先权。

须冻结parse_*的调用信封约定：原始输入保留外层调用定界和span，parse_*只解析该调用信封内部，嵌套组不剥。例 `twice![21]`解析内容21，而 `inc!{[1,2]}`内容仍是数组Expr。此项没有执行来猜实现行为。

CT公共实现按root决定采用独立sidecar、自有CtTy/闭合op/CFG；复用已有AST/名字解析、标量算术和控制流算法，不以Unit擦除，也不制造第二种函数/控制流语法。没有用户未决定才能继续的设计问题；以上API/锁细节推荐由root一次冻结后，消费者再把此套复制为正式输入。
