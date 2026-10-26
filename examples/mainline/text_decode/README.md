# 普通ZIL文本解码与UTF-16输入准备

四个模块分别负责动态字节追加、UTF-8逐标量解码、文本索引/UTF-16准备及有界输入消费者。输入从stdin到EOF，最多1024字节；消费者为本轮验证提供32个UTF-16单元的输出槽，库函数本身接收任意合法长度的借片。

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/text_decode/main.zil --entry value --opt O2 -o out/text-decode
./out/text-decode < input.txt
```

输出是`Unicode标量数*10^12 + UTF16单元数*10^9 + 单元校验值`，校验值逐单元计算`(hash*33+unit)%10^9`。非法UTF-8返回`-(错误序列起始字节位置+1)`；输入过长为-2000、分配失败为-2001、输出槽不足为-3000。空文本输出0；NUL是合法数据，不隐式添加或删除终止符。

`text.write_utf16`先完整检查输入和输出容量，非法输入或容量不足时不修改输出。补充平面标量通过两个surrogate单元表示；`text.byte_position`按Unicode标量映射字节位置，包含末尾光标。它不声称提供grapheme分割、规范化、双向排版或字形布局，也未把动态UTF16视图开放为可被外部长期留存的`c_wstr`。

编译器增量只有现有i32/i64位移的核验和机器支持；上述文本算法是普通ZIL源码，无C/Rust文本实现参与产品执行。原始源码、CT/raw/wire反例和O0/O2输入输出记录见`evidence/impl/124`。`bytes_truncate`与完整String拥有类型仍是后继独立能力。
