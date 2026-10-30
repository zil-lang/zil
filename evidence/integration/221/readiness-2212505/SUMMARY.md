# 固定组合候选 2212505 的 217 探针接续审计

本次只读重放绑定 `2212505a667508dc9c7565cbab674c8e59700457`。219 独审提出的 grouped module alias trace／阶段错误修复尚未包含，不能把本结果标成修复后的最终源。

- release CLI SHA256：`f9e5e62d501e5fbb458c56d3eed162187af782190dd96db588d62a7c05ded613`。
- 201 个生产输入的组合 SHA256：`6052f788be666554e5e9e20881fa7eb65c514330e40e2b3ea4d72f57a0d38c36`；逐文件清单见 `BUILD-INPUTS.json`。
- 原 PROBES.json SHA256：`1f25af9895df1f0bf0f5ae8000b8e262f3462634c41e40300561ece97017868b`；29 份原输入均核对冻结源摘要。
- 独立 target：`targets/218-integration-release`；Rust 1.99.1、LLVM/Clang 23.1.4、jobs=1、offline。构建前后 HEAD、clean 状态和全部生产输入一致。

## 实际变化

| 分类 | 217 原观察 | 当前观察 |
| --- | ---: | ---: |
| Verify Success | 11 | 13 |
| Unsupported | 16 | 14 |
| 保留的非法语法 SafeReject | 2 | 2 |

只有 `ct-item-if` 和 `ct-statement-if` 从 Unsupported 晋升到 Verify Success，各 `verification_runs=1`、exit=0。25 个其他合法输入观察保持；另外 2 个非法语法输入也保持。这里的 25 包括原 11 成功与当前 14 Unsupported。

`dyn-parameter` 和 `syntax-extension` 的原字节均保持；它们在 Parse SafeReject，不参与功能缺口结论。可用于能力观察的是另列的 `dyn-valid-parameter` 和 `syntax-valid-alias`。

## 仍为 Unsupported 的原输入

下表陈述这些固定源码的实际分类。它不证明所有替代表达都不可用，也不自动建立 ZUI 开工硬门槛。

| 原探针 | 有限形状 | 阶段 | 实际诊断 |
| --- | --- | --- | --- |
| `bytebuffer-slice-capture` | ByteBuffer 切片借入捕获 | types | slice capture or use in a closure is not implemented |
| `callback-return-generic` | 泛型 F 原样返回（218 本片仍关闭） | expand | escaping callable result is not implemented |
| `closure-owned-error-result` | 原拥有错误结果闭包形状 | types | higher-order scalar conversion is not implemented |
| `closure-package-param` | 闭包直接 Package 参数 | types | closure parameters require a finite Copy type or one Copy/slice address layer; address parameters require a Copy result |
| `optional-parameter` | 原生 optional 参数类型 | types | parameter type Unsupported("type form is not yet lowered") |
| `generic-lifetime` | 泛型 lifetime 参数 | expand | generic bound, lifetime or const parameter requires its semantic solver |
| `product-store-element` | Product 作为 Store 元素 | types | Store elements require nonzero complete direct-Managed Packages |
| `product-owning-store` | 原 Product 拥有 Store 字段形状 | types | aggregate field requires finite Copy, an explicit package manages resource, or a complete fixed Package array |
| `nested-store-element` | 嵌套 Store 元素形状 | types | Store elements require nonzero complete direct-Managed Packages |
| `owning-enum-option` | 非 Copy 拥有 Enum 载荷 | types | enum payload requires a finite Copy value |
| `associated-type` | 关联类型／非有限 trait 项 | types | associated trait items are not in the static local trait slice；trait declarations outside the finite local subset |
| `product-error-union` | 原 Product 错误联合形状 | types | error union payloads require finite Copy values or complete owned Packages; direct owned resource success requires explicit mode and a Copy error |
| `dyn-valid-parameter` | 修正语法后的 dyn 参数 | types | parameter type Address { mutable: false, inner: Unsupported("type form is not yet lowered") }；resource/address parameter mode |
| `syntax-valid-alias` | 修正语法后的 Syntax 导入／别名 | expand | public reexports and syntax imports require a later module interface |

`callback-return-generic` 原源码是 `fn keep<F:Fn()->i32>(f:F)->F=>f`。本次没有将它替换为 fresh wrapper；其继续拒绝与 218 冻结范围一致。

## 单独的 218＋219 交互控制

`combined-control/main.zil` 在模块级 #if 中生成公共 `wrap<F:FnOnce>`，在工厂体的 #if 中绑定 bias，在调用方的 #if 中绑定返回环境。实际传入 Fn 环境，经声明 FnOnce 包装后调用并得到 42。它与 29 份原探针分列。

源码 SHA256：`d8d716317d83258a47edd1bab8f06ea426ec4410686dd2374df4ef7ed98b7383`。先冻结 `ORACLE.json`，再执行一次 CLI check 和 O0／O2 build＋native run；三次编译各 Verify 一次，两个运行均 exit=0、stdout/stderr 为空。源、完整阶段输出、selection、TypedIR、verified interfaces、LLVM 和可执行文件摘要均在 RESULT 与各命令目录。

这些执行不覆盖分配失败、并发、框架或完整 ZUI。29 原探针只有 check 分类证据；不得把 13 个 Verify 成功标成 13 个 native 消费者。

## 可核对交付

- `BUILD.json`、`BUILD-INPUTS.json`、`build.log`：固定构建身份与命令。
- `RESULT.json`：29 项原预期、实际分类、诊断、每个源码摘要和组合控制。
- `original-PROBES.json`：原预期字节副本，未改写。
- `replay.py`：实际重放脚本；每个 `probes/<name>/command.json` 保留完整参数、exit 与 stdout/stderr 摘要。
- `combined-control/`：预先冻结的源／oracle、check 和 O0／O2 构建运行证据。

只读任务完成；没有修改生产树、原探针或主树工单，没有运行全量或产品门。
