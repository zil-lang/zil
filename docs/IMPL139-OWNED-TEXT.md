# 139：普通 ZIL 的失败保持原地文本编辑

## 为什么

132已有UTF8校验和新缓冲区重建版`replace`，但每次编辑要重新持有并提交一个ByteBuffer。
本片利用137已恢复的`bytes_truncate`，完成普通ZIL库`replace_in_place`，不再为了缩短/等长编辑重新分配载体。
未新增语法、运行时引用计数、隐藏缓冲区或公共IR；132校验器和复制版按原源码恢复，不算本片新发明。

## API 与求值顺序

```zil
import edit
let text=bytes_new()
let insertion=bytes_new()
// ...填入合法UTF8字节...
let result=edit.replace_in_place(text,begin,end,bytes_view(insertion),byte_limit)
```

`begin/end`是Unicode标量边界，不是字节索引、字素或视觉光标。顺序：校验完整来源 → 校验完整插入 → 校验范围/溢出/长度上限 → 必要时reserve → 搬移 → 复制插入 → 缩短初始化前缀。
`code=0`成功；1来源编码错、2插入编码错、3范围错、4上限/溢出、5分配失败。1/2各报告第一个非法标量的起始字节。1..5保持原字节、长度、容量和责任不变。

向右搬移从后到前，向左从前到后，允许内部源目标区间重叠。增长先确保容量并初始化扩出的字节，然后才借出完整可变视图；缩短在视图结束后才truncate。由边界不等式保证reserve成功后的push/truncate没有可恢复失败分支；不把未初始化容量当作已初始化切片。

插入视图必须与目标实际后备存储分离；同一个buffer的self-insertion被独立Verify拒绝，不自动复制。旧目标视图不能跨越编辑；已证明分离的Package另一字段视图可保持可用。捕获拥有buffer的FnMut可重复编辑而不更换载体。

## 本轮真实执行

工具：Rust1.99.1、Linux x86_64、Clang17.0.0；本轮重建137候选，不复用旧测试成功标记。

* Python独立UTF8期望：1708个输入（合法/非法、标量边界、NUL、随机字节/文本），O0/O2及其分配探针版，共6832次实际执行全部通过。
* 10次分配失败/无分配探针：准备失败、增长reserve失败保留原值、缩短时武装后续失败也不会触发；各实际已分配责任都释放，live=0。
* 4个Rust集成测试：Package分离字段、拥有FnMut重复调用、i64极值/坏尾部/失败不变、旧窗口和自插入反例；实际O0/O2正例6次。
* 同一编译器和库，1000次等长编辑，分别修改头/中/尾：复制版1002次allocator调用，原地版2次，包含双方相同的2次初始化。O0/O2均核全部最终字节，live=0。
  武装第3次分配失败时，复制版第1次编辑失败，原地版仍完成1000次。它是确定性分配次数证据，不是墙钟/Rust性能结论。
* 最后又复跑ByteBuffer、owned bytes、storage反例、137与139的7个定向测试目标，共47项全部通过。cargo fmt和新增测试/137测试目标Clippy -D warnings通过。没有运行正式全量门或独立审核。

## 保留的失败与边界

重复编辑对照初稿把范围推成i32，Types正确拒绝；补显式i64后又遇到当前普通Managed局部完整替换Unsupported。
最终两臂都使用已有可写Package字段作为载体，字段替换遵守现有准备/清理/提交合同；没有修改安全核去绕过限制。
原失败报告都保留。普通Managed局部整体替换涉及显式清理与逆初始化自动清理的区分，不能只用DropManaged拼接Store冒充完成。

库不是完整String类型，ByteBuffer仍可被其它API写入非法UTF8；这里只保证每次成功编辑输出合法。没有grapheme、规范化、排版、通用Vec<T>、任意资源/错误载荷或异步。

## 四目标核对

安全：沿独立Verify，加入别名/失败/边界负例，非完整语言安全证明。
运行：已证这些场景的分配消除；未测墙钟/Rust对比。
表达：普通函数即可组合包、借片和拥有回调；不是新语法优势证明。
编译：本片没有编译速度结论；正式产品四目标仍未达成。

## 复跑

```sh
cargo test --locked -p zil-driver --test impl137_truncate --test impl139_owned_text
cargo build --locked -p zil-cli
python3 tools/research/139/native_test.py --driver target/debug/zil --out /tmp/zil139-native
python3 tools/research/139/repeated_edits.py --driver target/debug/zil --out /tmp/zil139-repeat
```

输出目录必须不存在；使用其它target目录时修改`--driver`。探针使用Clang的Linux linker wrap，仅用于测试，不是库实现。
