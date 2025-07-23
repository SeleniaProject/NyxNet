# Key management messages
rotate-success = Key successfully rotated.
quarantine-added = Node { $node } added to quarantine list.
quarantine-duplicate = Node { $node } is already present in quarantine list.

# Connection management messages
connect-establishing = Establishing connection to { $target }...
connect-success = Successfully connected to { $target } (Stream ID: { $stream_id })
connect-failed = Failed to connect to { $target }: { $error }
connect-timeout = Connection timeout after { $duration }
connect-interrupted = Connection interrupted by user
connect-daemon-unreachable = Cannot reach Nyx daemon. Is it running?

# Status command messages
status-daemon-info = Daemon Information
status-node-id = Node ID: { $node_id }
status-version = Version: { $version }
status-uptime = Uptime: { $uptime }
status-traffic-in = Traffic In: { $bytes_in }
status-traffic-out = Traffic Out: { $bytes_out }
status-active-streams = Active Streams: { $count }
status-peer-count = Connected Peers: { $count }
status-mix-routes = Mix Routes: { $count }
status-cover-traffic = Cover Traffic Rate: { $rate } pps

# Benchmark command messages
bench-starting = Starting benchmark against { $target }
bench-duration = Duration: { $duration }
bench-connections = Concurrent connections: { $count }
bench-payload-size = Payload size: { $size }
bench-progress = Progress: { $percent }% ({ $current }/{ $total })
bench-results = Benchmark Results
bench-total-time = Total time: { $duration }
bench-requests-sent = Requests sent: { $count }
bench-requests-success = Successful: { $count }
bench-requests-failed = Failed: { $count }
bench-throughput = Throughput: { $rate } req/s
bench-latency-avg = Average latency: { $latency }
bench-latency-p50 = 50th percentile: { $latency }
bench-latency-p95 = 95th percentile: { $latency }
bench-latency-p99 = 99th percentile: { $latency }
bench-bandwidth = Bandwidth: { $rate }
bench-error-rate = Error rate: { $rate }%

# Error messages
error-invalid-target = Invalid target address: { $target }
error-daemon-connection = Failed to connect to daemon: { $error }
error-network-error = Network error: { $error }
error-timeout = Operation timed out
error-permission-denied = Permission denied
error-invalid-stream-id = Invalid stream ID: { $stream_id }
error-stream-closed = Stream { $stream_id } is closed
error-protocol-error = Protocol error: { $error }

# General messages
operation-cancelled = Operation cancelled
please-wait = Please wait...
press-ctrl-c = Press Ctrl+C to cancel
completed-successfully = Operation completed successfully
warning = Warning: { $message }
info = Info: { $message }

# Table headers
header-error-code = Error Code
header-description = Description
header-count = Count
header-stream-id = Stream ID
header-target = Target
header-status = Status
header-duration = Duration
header-bytes = Bytes 