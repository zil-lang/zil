# 325补充：历史候选夹具结构迁移到Wire34

root首轮全仓在历史Wire32夹具处报E-WIRE-VERSION。只迁移测试输入及其消费者，
生产wire.rs、兼容边界、版本号和安全oracle不改；不新增Cargo测试目标。

临时工具在`/tmp/zil-wire32-migration`使用现有base/ir作为公共类型。复制当前
wire.rs，临时legacy module改WIRE_VERSION=32，去掉函数reference_return_bound
字段的写出/读取，并在解码结构中明确设None；四个私有tag helper直接取现有IR
实现。legacy module SHA-256为`edaa8192da853623850be4366866739efca7e1136faf80d0f6d8393ee5e1773d`。
不增加生产legacy decoder，不把修改版本字节当结构迁移。

对每份输入依次证明：

1. legacy32完整decode→encode逐字等于原文件；原始schema/claims均为32。
2. 在解码结构上把module/plan schema置34，target规则取当前RULE_VERSION，
   刷新module/target fingerprints；所有函数reference_return_bound仍None。
3. current34完整encode→decode等于新module和plan。撤回仅schema/target/指纹
   元数据后，新旧结构逐字段相等；新文件长度恰增加函数个数，即真实编码None位。
4. current decoder对每份原文件仍报E-WIRE-VERSION。

共17份新增v34：A=Analyze impl253组11份，B=tests/impl254组5份，
C=driver review223组1份。三组历史根目录共21份旧二进制均与Git逐字相同，
包括保留的v27/29/30/31；没有覆盖、删除或移动。A的11份仍按原4正/7负安全
oracle实际Analyze；B/C原Verify及源码重建oracle保留。review223的旧版本拒绝
增加保留的32，并直接消费结构迁移后的34，不再以旧offset拼当前封套。

| 组/旧文件 | 新文件（组内v34/） | 旧SHA-256 | 新SHA-256 |
| --- | --- | --- | --- |
| A/control_concrete_copy_byvalue_call.wire | control_concrete_copy_byvalue_call.wire | 1157bf56fc92d28cff3583bcec7a9f3bb82c4e122ef2e574393fbb791ac834cf | 64d8c893c298e29712c55c70df6724430a777f24597e3d240eeb158c6c2575a8 |
| A/control_shared_callback_i32_argument.wire | control_shared_callback_i32_argument.wire | 863cd025955b3770549e6394df1a7e90233bbe718fb74d62c94582e8445da632 | dc2424a0450cb6c52a9dde26b95449326deb253d191fb7b26953a131aec26b26 |
| A/escape_byvalue_symbolic_fn_call.wire | escape_byvalue_symbolic_fn_call.wire | 9f24b943fde097df29e9d16c2a62aa6b7d5250fe7e5754ebe47ad3a28bd8a904 | 0f1f8e4f98007f1babff6bd2d28ea89d2281543786f3aff90185a5f4661f47e2 |
| A/escape_callback_argument.wire | escape_callback_argument.wire | fb2178ef091876fa07a18641425aa681ac422b04dfc366aebca4a908123b9ca4 | f1e52db82fe5ebce525e929571698f627048c3397cc8808e16d4ebc2601f7752 |
| A/escape_shared_callback_f_argument.wire | escape_shared_callback_f_argument.wire | 3403ed323e0b269150787f1271dbcd1cd6ca717e1d337a452ad1ace6caa4e30d | 74891a0461b53d66e9587fd9696458fdd00a08fd3212a81c1ead94536f0fc810 |
| A/escape_thin_store.wire | escape_thin_store.wire | 24939acab8c288418996ffedf8d8b150fcd0ee9cc7cf628fd5f1e8e06760a672 | 769c7a3ec794779841fb1287247a82267c215e7a914063a4bc4022d3c638b30f |
| A/escape_two_element_array.wire | escape_two_element_array.wire | b5d2f4195e090a0d777588727703dc0ebabd82597b8a66400f6364078efde9e0 | adad8f8d8e76f8f49acba46b129d600f210ec1f82a6b709c6957af07dbf2f581 |
| A/parent_stale_no_call_edge.wire | parent_stale_no_call_edge.wire | d476eaed4c31e8c95068e6aa06c3bd79980c2ae23ebb89b515fce2f15d65bc0c | 51f4df364ca6640de50a54816749baacd97330439f0b164bd5e2baa658d01459 |
| A/parent_stale_with_call_edge.wire | parent_stale_with_call_edge.wire | 71818b07ebc5606356bd091d6467e205675eadc5b48ec73625511830abac3520 | 8db877b7d92469fa131dcf986596bdaa698680a6037512971cdae26f4f82ad26 |
| A/parent_valid_no_call_edge.wire | parent_valid_no_call_edge.wire | d9b4c449d963ea911e7686b730dc6d18e615e5b79550f0e7844e8f3c4a1a84fb | f29d965cad34b82c148c25c8c12599e336b4878ba6463d97e4c161657b287ac0 |
| A/parent_valid_with_call_edge.wire | parent_valid_with_call_edge.wire | f6b62ef9f79bb05443ba9923571803ba4f09ab951921d01f9683b6b858015a5d | 141a372abd997032bf94ba25d80b6aedef65211acd79be1c8780fc26bc5bf580 |
| B/control_callback_i32_argument.wire | control_callback_i32_argument.wire | fabdeca50fa9b43cf17a6885da7644854265086cd029509f45b9df3f924fd14a | 4aefe2b4d60ddd5c7f685cbcacc3f70ee7fe255f2a00a5c0d88e029ace530061 |
| B/escape_callback_argument.wire | escape_callback_argument.wire | ea6d8ff6396911ea614bd4c360962d41f1765ba3b94bd4cf89fb327ec96f036b | bad82bb1d10c291bca463be21b9ee229160229dfe81e9ca259eef15625ba3ab2 |
| B/loan_revival_v32.wire | loan_revival.wire | 03ee143e02db7f4b076e7288eaefc50d1efd0da7ab69f40314e4e40d116e0823 | ab8801c65d2bbed13d2b64bbb6df7c25dbd41f78c5784c93be6fabc8aaeab0bf |
| B/parent_valid_no_call_edge.wire | parent_valid_no_call_edge.wire | cc3ff6b63f2f36745eb2a324a0508f07181f89cbaa31858c3ec04582e62bd060 | 5e17116f7d90b2c57dd8eddf8cc038454fca180af6a7ec31ba3d56fa9c92c1d7 |
| B/parent_valid_with_call_edge.wire | parent_valid_with_call_edge.wire | ae83ff58daf1b01a3a563332c334a5c89fb1041474feb74c3c3e528a760cba77 | 4fc368db497d6940ca491574545ddb3c7574566fe4e4571ff810290106c9da2e |
| C/schema32-mutable-product.wire | mutable-product.wire | 8cc0e4a565e57b71135b3c692079dcaa65dfa4ce6f49dbe36c253182df497862 | cf116f5aa5bd4a2cf69f2beeff265080ce9b5154d31a8918b79ca0478e7a5a51 |

实际迁移命令使用临时Cargo manifest（offline、target325、jobs1）分别输入上述
三组目录、输出对应v34目录；未运行全量。限定检查如下（driver显式llvm-api）：

- `cargo test -p zil-analyze --lib forward::tests::reviewed_ -- --nocapture`：2/0，覆盖原11夹具及旧版本拒绝。
- driver既有impl254_capture_probe、impl254_operands、impl254_wire、review223_replay四目标：共5/0。
- impl238_product_replacement仅review318_frozen223_product_parameter_has_real_exclusive_authority：1/0。
- impl248_source仅callback_return_does_not_revive_a_borrowed_input_window：1/0。

共9个受影响既有测试通过；正/负安全结论及实际Analyze/Verify仍核验，不以解码
成功代替安全。此次只有历史测试运输格式迁移，不增加语言/平台/并发能力，
不更改Rust产品四目标结论。正式全量由root重新执行。
