# Full-O2 producer：2458精确delta独审

**无当前pure、无cache固定研究范围的blocking issue，可继续受控构建及原两case验收。** 本审只读取Git diff、源码和小文件SHA，没有运行工具链、LLVM/native或计时，也没有预填尚待执行的控制/质量/性能结果。

审查作者clean local `6706af480cdcdd9fe3e8fb92a7f70ee87b389150`，tree `e917926bc8976da90e58897911c72978b9a24f88`，精确对父`2f55670`（对应旧remote850289f5）独审。Root随后发布remote `2458defcaf8be51c87b80c0dadf391bb585735e8`，同tree；未来producer身份使用remote2458，不把同tree写成同commit。

| 文件 | 实核SHA256 |
| --- | --- |
| `native/research_prelink.rs` | `4d306a3d0da65ee0cf6fcd34904b82db6e7713ecdcc172592717920ed5795680` |
| `lib.rs`（只有docs变化） | `f116bed15e2527598b9b93dc05e00d06ab79822ef8d21934005af31c2e977046` |

## 源码路径

producer实际由`thinlto-pre-link<O2>`改成完整`default<O2>`，原machine preflight、builder、前后LLVMverify、纯模型扫描、canonical定义集合及类型检查均保留。新增优化后每个canonical再次必须External0，阻止“有body但并非规范可发射owner”的误认；新global/alias/未知定义等仍按旧scope拒绝。sealed bytes/facts由这次实际full-O2 module生成，无旧receipt/cache输入。

目的端保留原全图owner/foreign/签名校验、递归actual Required计算、Required⊆Allowed、真实own parse/verify、target/layout及check_sealed。只有这些全部成功后才取`postlink_optimization_ran = !required.is_empty()`，不看Allowed、旧source calls或收据。空分支不修改module，trim/link/private/第二优化皆不执行，link_ns和optimize_ns准确为0；随后公共LLVMverify、取消检查、既有emitter及最终native link继续执行。原空路径中的coverage检查已由check_sealed+canonical==definitions等同闭合，不需复制一轮扫描来证明同一空集合。

非空分支仅把原代码包入else：donor DCE前后verify及selected精确集合、消费源的同Context链接及每次精确defs、全量Use/type检查后统一私有rename、变更后verify、完整thinlto<O2>均保留，公共verify覆盖优化后结果。FFI/RAII、typed原预算和backend接线没有变化；不加新attrs、阈值或策略selector。

## 收据与控制

unit/object改为版本2；producer明确`pipeline=default<O2>`，object新pipeline明确empty-required=emit/nonempty-required=thinlto<O2>。旧笼统`verify_parse_trim_link_private_postlink=true`改成实际`actual_required_path_verified`及`postlink_optimization_ran`，避免对空路径声称未运行的阶段已经运行；donors/imports/ns的空值与实现一致。这不是原2321相同pipeline或相同机器码声明。

原四项真实CAPI控制保持数量，八unit控制增强了两个路径：f0→f1仍实际Required非空，断言私有导入、postlink_ran=true与正optimize_ns；f2原模型有dead f3调用及真实getchar，full-O2后图只剩getchar，Allowed仍含f3而actual Required为空，断言ELF、无imports/donors、ran=false及0ns。这能区分“按实际Required”与错误的“按Allowed/有无任何call”。缺Allowed、合法callee兼参数地址Use、BitReader/Link失败所有权控制继续存在。本审静读未执行，不能代报新源码tests通过。

## 其他四文件及构建隔离

差异只涉及五文件：lib docs、native child、FFI ownership说明、研究README、新`build-full.py`。Config/Receipt/model、backend预算及依赖未改。文档清楚保留旧NoGo，要求新源独立实测。

构建recipe先核exact clean commit/tree，要求新target不存在，从旧target用`cp -a --reflink=auto`复制（没有hardlink选项），再真实cargo tests/proof/no-native/release；前后核seed binary身份，不能把旧binary当新编译结果。Root负责新target/manual-full-2458和独立raw，旧ce906保留；本审未启动recipe。异常/失败不能产生Pass binding，部分原process attempt仍由既有helper保存。

## 剩余实证范围

full producer可能提前改变inline/向量化/attrs与导出体形状，且其自身可能比thin-prelink更贵；空路径省一个优化调用不保证完整请求净收益。新结果必须沿固定两case、完整请求wall/CPU与原1.10/.90运行门测量，不以旧stage总和扣除估算，不扩到global/address/cache/many-edit或生产采用。
