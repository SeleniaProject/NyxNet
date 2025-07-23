# 密钥管理消息
rotate-success = 密钥轮换成功。
quarantine-added = 已将节点 { $node } 添加到隔离列表。
quarantine-duplicate = 节点 { $node } 已存在于隔离列表中。

# 连接管理消息
connect-establishing = 正在建立到 { $target } 的连接...
connect-success = 成功连接到 { $target }（流ID: { $stream_id }）
connect-failed = 连接到 { $target } 失败: { $error }
connect-timeout = 连接在 { $duration } 后超时
connect-interrupted = 连接被用户中断
connect-daemon-unreachable = 无法连接Nyx守护进程。它在运行吗？

# 状态命令消息
status-daemon-info = 守护进程信息
status-node-id = 节点ID: { $node_id }
status-version = 版本: { $version }
status-uptime = 运行时间: { $uptime }
status-traffic-in = 入站流量: { $bytes_in }
status-traffic-out = 出站流量: { $bytes_out }
status-active-streams = 活动流: { $count }
status-peer-count = 连接的对等节点: { $count }
status-mix-routes = 混合路由: { $count }
status-cover-traffic = 掩护流量率: { $rate } pps

# 基准测试命令消息
bench-starting = 开始对 { $target } 进行基准测试
bench-duration = 持续时间: { $duration }
bench-connections = 并发连接数: { $count }
bench-payload-size = 负载大小: { $size }
bench-progress = 进度: { $percent }% ({ $current }/{ $total })
bench-results = 基准测试结果
bench-total-time = 总时间: { $duration }
bench-requests-sent = 发送请求数: { $count }
bench-requests-success = 成功: { $count }
bench-requests-failed = 失败: { $count }
bench-throughput = 吞吐量: { $rate } req/s
bench-latency-avg = 平均延迟: { $latency }
bench-latency-p50 = 50百分位数: { $latency }
bench-latency-p95 = 95百分位数: { $latency }
bench-latency-p99 = 99百分位数: { $latency }
bench-bandwidth = 带宽: { $rate }
bench-error-rate = 错误率: { $rate }%

# 错误消息
error-invalid-target = 无效的目标地址: { $target }
error-daemon-connection = 连接守护进程失败: { $error }
error-network-error = 网络错误: { $error }
error-timeout = 操作超时
error-permission-denied = 权限被拒绝
error-invalid-stream-id = 无效的流ID: { $stream_id }
error-stream-closed = 流 { $stream_id } 已关闭
error-protocol-error = 协议错误: { $error }

# 通用消息
operation-cancelled = 操作已取消
please-wait = 请稍候...
press-ctrl-c = 按Ctrl+C取消
completed-successfully = 操作成功完成
warning = 警告: { $message }
info = 信息: { $message }

# 表头
header-error-code = 错误代码
header-description = 描述
header-count = 计数
header-stream-id = 流ID
header-target = 目标
header-status = 状态
header-duration = 持续时间
header-bytes = 字节数 