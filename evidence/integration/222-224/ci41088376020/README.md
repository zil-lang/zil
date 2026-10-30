# 原始 CI 回执归档

本目录完整保留原始审计和回执。受单次传输容量限制，压缩归档按字节分片存储；没有删减、重新执行或改写任何原始证据。

运行 python3 restore-raw-receipts.py 按顺序恢复 raw-receipts.tar.gz。脚本先核对每片大小与 SHA256，再核对合并后的原始归档大小与 SHA256；核验成功后可用 tar -xzf raw-receipts.tar.gz 展开。恢复后的文件清单与逐文件摘要见 artifact-manifest.json。AUDIT.json 保持原字节。
