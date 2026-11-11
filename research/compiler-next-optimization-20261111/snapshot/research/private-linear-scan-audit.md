# Private wrapper逐import重扫：复杂度与等价性静审

## 结论

`hook_checks.check_hooks`现有direct-Use计数确实对每个实际AE import重新执行整份before IR的`splitlines`、逐行去注释与`SYMBOL.findall`，工作量为 **O(imports × IR字符量)**。把相同token的出现次数一次累加到Counter，再逐canonical读取`count-1`，在保留相同逐行切分、注释处理、正则及重复次数的条件下代数等价；不会减少任何Verify、机器预检、LLVMverify、body可见性、owner/copy预算或插件变更证明。

当前只读结论支持新独立ROOT的这个最小修改，不表示新版本编译已获益。旧identity完整36次screen应永久保留：两case质量Pass而编译NoGo，N64 B/A wall3.0335296、CPU2.1056227，N256 wall2.5076075、CPU1.8153869；不能将此前未分解时间都归给此循环。本审没有编译、LLVM/native执行或计时。

## 实际循环量

原始数据来自`out/next-optimization/private-identity-screen-v1`六轮、两个case、每轮8个模块；逐文件行数/大小/hash及计数在`research/private-linear-scan-evidence.json`。共读取96份真实before IR。每轮只有unit0含AE imports，另七unit没有，不能为了“统一实现”给它们新添一次whole IR计数。

| unit0每轮 | N64 | N256 |
| --- | ---: | ---: |
| 实际imports I | 56 | 224 |
| before IR行数 L | 12778 | 51547 |
| before IR字节 | 564691 | 2284578 |
| definition headers H | 66 | 258 |
| 去注释后符号出现总数 | 1141 | 4576 |
| 原循环regex调用 I×L | 715568 | 11546528 |
| 新单次扫描regex调用 | 12778 | 51547 |
| 旧`.count`累计token访问 I×符号数 | 63896 | 1025024 |
| 六轮旧regex调用 | 4293408 | 69279168 |
| 六轮新regex调用 | 76668 | 309282 |

regex调用减少56/224倍不等于完整wall减少这些倍数：仍有一次真实扫描、Counter分配/累加、其他Python检查、首次1.031GB工具身份读取、ZIL/frontend/planner、LLVM/O2/emit/link以及文件I/O。当前没有该循环独立计时，不能给它分配多少毫秒。

## 严格等价边界与最小写集

旧表达式是：

```python
uses = sum(SYMBOL.findall(line.split(';', 1)[0]).count(canonical)
           for line in before_text.splitlines()) - 1
```

令每行token序列为T(line)，则旧结果为`Σline Σtoken∈T [token==canonical] - 1`；Counter正好累计同一个有限和。必须保留：

- 同一个`SYMBOL`正则、`splitlines()`及`line.split(';',1)[0]`；不能对全文直接findall而包含注释，也不能另造strip/metadata/body-only规则。
- 每次出现都计数，不能转set；同一行重复symbol也保留。缺失key必须是0，从而依然返回-1，不能为缺失canonical悄悄跳过。
- `-1`仍只扣单个definition header。前置`parse()`、AE与actual.3一致、变更名单完整和后续私有definition/fullmodule逆归一化全部保持，不用Counter替代only-direct或LLVM验证。
- `changes`非空才建立Counter；数据局限当前unit，不能跨unit/request复用。

最小写集就是新ROOT的`hook_checks.py`导入Counter、在已核`changes`名单后生成一次token计数、把旧表达式换成lookup；其他冻结工具/fixture/协议仅按独立新ROOT正常生成新的script identity。无需改C++、Rust、预算、阈值、target或运行质量门。Counter预计算不产生新语义检查，原有错误判定顺序可保留。

本次独立离线统计对96份before IR建立计数，所有实际canonical的`Counter[name]-1`都与真实C++逐Use收据`direct_uses`相等。该收据来自不同实现的LLVM Use遍历，提供真实数据交叉核；一般输入的等价性来自上述有限和证明，不把旧正则重跑当新的LLVM/native证据。

## 另两个确定的同类重复（建议分开施工）

### `_normalize_header`逐definition遍历AE集合

现实现每个`define`行用`any('@'+name+'(' in line for name in selected)`查目标，前后各执行一次；是O(H×I)字符串检查，而不是whole IR regex重复。当前parser已要求每个header只有其自身一个symbol，因此可以复用同一HEADER正则提取该行定义名并以set membership判定。

必须保持原`line.startswith('define ')`范围、返回的原始换行及原header归一化规则，不因便利而strip整行或接受原来不归一化的缩进header。对当前真实unit0，全部I个目标都有header，精确两遍比较量为`I(I+1)+2(H-I)I`：N64每轮4312次、N25665632次。它比direct-Use regex问题小，不建议未经新freeze悄悄叠入最小Counter对照。

### `visibility.check_modules`每个function重建全体owner集合

`owners`在该函数内不变，但prelink和actual-import逐function检查反复构造`set(owners) | valid_external`。同请求调用check_modules两次：prelink-only一次、post-import一次；可在每次调用开始构造一个`known_symbols=set(owners)|valid_external`，之后复用。纯成员关系等价，不能替换Required closure、Allowed或原预算检查。

从真实prelink manifest和actualvisibility计数，N64完整owner572、prelink定义P572、actual.3定义Q122；N256分别2300、2300、482。两次check_modules中这种owner set构造为`24+2P+Q`：1290/5106次，至少复制737880/11743800个owner key，尚不含union的第二次复制。这个构图热点独立于Counter；并非LLVM必须工作，也不是停用proof的理由。

另有`canonical not in unit['allowed_original']`对列表逐项查找，可局部预建Allowed set，但当前只有56/224次查找，优先级低。原manifest/currentproof/IRBC的实际SHA和before/after全文检查有明确证据用途；没有据“重复”把它们一并省去的授权。

## 后续判定

新ROOT保留旧36screen正负证据；等冻结的新完整screen实际测wall/CPU，再判定Counter是否有足够净收益。若仍NoGo，如实报告，不通过算术扣diagnostic成本、改变编译/运行样本或放宽质量门。只有后续单独授权才把次级集合/normalizer热点加入另一个明确候选。

## 新源码精确小审

已读取`research/thinlto-private-linear/hook_checks.py`实际字节，SHA256为`520241d39255ec74f319697e6b865a0533e7a66958ee99cff995b025d7bd77f9`。与旧identity文件的diff只有Counter导入、`changes`非空时同`splitlines`/逐行去注释/同SYMBOL的一次累计，以及`uses=references[canonical]-1`。无imports时创建空Counter而不扫描文本；重复token保留、缺失key仍为0。与本文等价证明一致，无当前源码阻断。

目录独立diff确认：其余可执行源码仅`common.py`/`prepare.py`/`run.py`/`compile_screen.py`将protocol/result format域从identity改为linear；CPP、wrapper、processes、visibility、vendor fixture及全部验证流程字节不变。README只补旧NoGo和本次机制说明；旧prefreeze/result-review没有冒充新ROOT运行结果。新增normalizer/owner-set优化没有叠入。

此次是源码冻结前精确小审，不替尚待生成的新protocol与完整screen结果。未编译/LLVM/native/计时，也没有改候选源码。可以继续冻结原36完整cold编译、108native/36formal、完整检查、原1.10/.90门；不得预称编译收益。

## 实际protocol gate：Pass

准备后的`out/next-optimization/private-linear-protocol-v1/protocol.json`实际SHA256为`a30ffe86586c77849a613be636ece4f58fa3bfcebbc0eec3191cba333ce4e72c`。独立JSON逐顶层字段比较identity722a协议，**仅format与scripts不同**；完整cases、tools、proof/build receipts、orders、CPU/jobs、limits及其他所有字段均完全相同。没有读取或重hash大工具images。

14个protocol scripts逐本地小文件SHA全匹配、集合与实际Python/C++文件一致。再读取发布source `3d024219dc48f6ee1cec57c87afa2e62580a7ad6` 的`research/compiler-thinlto-private-linear/`15个Git blob（14 scripts加README），与本地`research/thinlto-private-linear/`逐字节相同；tree核为`cdeb311aee300549e797561d13ef76411fffdb90`。小文件静态protocol gate无阻断，可以按已冻结36/108运行，未预填结果。随后停止工具使用，等待唯一重窗口结束。
