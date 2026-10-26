# 输入与外部依据

当前规范的规则定义在 `_zil_design.md`；本文件只记录可追溯来源，不额外定义语义。

## 用户输入

- `zil-vale-v2.md`：语言、编译期、实现边界和验收组织的参考。
- `lang-designer.html`：需要原位改造的D2系统；原42目标、28变量及7种非ZIL语言数据保留。
- `zil-S6-design.md` 与 Rust开工包：描述/权限/责任语义及Rust模块依赖。
- `_zil-goal_vale.md`：旧开发工作流；不是当前规范与进度事实。
- 文件内容SHA-256见 `data/input-hashes.json`。

## 外部一手资料（2026-10-22读取）

- Rust Reference, Attributes: https://doc.rust-lang.org/reference/attributes.html — `#[...]` 是属性输入，存在内建、过程宏、工具等不同类别；不等于普通函数执行。
- C# language specification §23: https://learn.microsoft.com/en-us/dotnet/csharp/language-reference/language-specification/attributes — 方括号属性在语法上可行；C#还具有运行时元数据语义，ZIL未据此采用相同模型。
- Lean Reference, Notations and Macros: https://lean-lang.org/doc/reference/latest/Notations-and-Macros/ — 语法扩展、卫生、source位置及elaboration分层；ZIL限制扩展权限而不复制其全部元编程宿主。
- Zig Language Reference: https://ziglang.org/documentation/master/ — comptime、类型值、编译期参数与普通程序构造。master是移动目标，生产工具链必须另锁版本。

这些资料用于机制对照，不作为ZIL已实现、全语言健全性或编译更快的证据。HTML中其余语言的原有资料与场景判断沿用输入数据，并未在本轮逐条重新验证。


## 收尾语法核对（2026-10-22）

- Rust Reference，Closure types：https://doc.rust-lang.org/reference/types/closure.html 。核对默认捕获、move与Fn族分离、函数指针转换；ZIL的Desc/Managed组合为本规范设计，不是Rust原有语义。
- Rust Reference，Associated items：https://doc.rust-lang.org/reference/items/associated-items.html 。核对关联函数与显式receiver；组默认与词法前导点由ZIL定义。
- Zig Language Reference，struct：https://ziglang.org/documentation/master/#struct 。借鉴容器命名空间与普通函数的点调用，不声称Zig自动提供安全合同。
- MoonBit Method and Trait：https://docs.moonbitlang.com/en/latest/language/methods.html 。核对类型命名空间方法及接口可读性，未整套导入其托管内存模型。
- MoonBit Fundamentals：https://docs.moonbitlang.com/en/latest/language/fundamentals.html 。箭头/词法闭包及上下文推导作为人体工学参考，ZIL参数模式/错误合同仍遵守自身内核。

这些官方页面为机制参照，不证明ZIL安全或更快。离线图谱中的其他语言既有数据没有在本轮逐项更新。


## 第四轮公开原文（2026-10-23）

- 公开原对话：https://chatgpt.com/share/6adab6de-6818-8322-9940-a0defbcbd15b ，页面标题“ChatGPT - zil设计2”。逐个Prompt导航装载页面虚拟化消息，连续T01–T54共54条正文（27用户、27助手，206,587字符）完整；没有登录。附件本体未取回，不把助手的附件概述视为附件原文。
- 独立原文归档`shared-transcript-full.md`，484,813字节，SHA-256 `2811f9938176aaa56fcbb04a99a831acc68c23bf80b2a79a072fba920a9a208f`；配套`shared-transcript-capture.json`保留消息ID、正文、代码块及页面链接。它们在独立设计审计交付中，不增加日常源码包体积。
- T42为当时正式文稿裁决；T44/T46/T48/T50/T52明确仍属候选、未覆盖正式design/goal/designer；T53用户反对闭包负担；T54撤回捕获表、Copy默认快照与普遍give，建议自动捕获、move及组receiver。最后助手答复不是逐项用户批准。
- 本轮新增和保留规则见`changes/IMPLEMENTATION-0013.md`与`docs/DESIGN-RECONCILIATION.md`。&T?的候选结合修改已因用户要求继续讨论而撤回；当前实现和建议分开记录，不虚构使用频率。
