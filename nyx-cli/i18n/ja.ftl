# 鍵管理メッセージ
rotate-success = 鍵のローテーションに成功しました。
quarantine-added = ノード { $node } を隔離リストに追加しました。
quarantine-duplicate = ノード { $node } は既に隔離リストに存在します。

# 接続管理メッセージ
connect-establishing = { $target } への接続を確立中...
connect-success = { $target } への接続に成功しました (ストリームID: { $stream_id })
connect-failed = { $target } への接続に失敗しました: { $error }
connect-timeout = { $duration } 後に接続がタイムアウトしました
connect-interrupted = ユーザーによって接続が中断されました
connect-daemon-unreachable = Nyxデーモンに到達できません。実行中ですか？

# ステータスコマンドメッセージ
status-daemon-info = デーモン情報
status-node-id = ノードID: { $node_id }
status-version = バージョン: { $version }
status-uptime = 稼働時間: { $uptime }
status-traffic-in = 受信トラフィック: { $bytes_in }
status-traffic-out = 送信トラフィック: { $bytes_out }
status-active-streams = アクティブストリーム: { $count }
status-peer-count = 接続ピア数: { $count }
status-mix-routes = Mixルート数: { $count }
status-cover-traffic = カバートラフィック率: { $rate } pps

# ベンチマークコマンドメッセージ
bench-starting = { $target } に対するベンチマークを開始中
bench-duration = 実行時間: { $duration }
bench-connections = 同時接続数: { $count }
bench-payload-size = ペイロードサイズ: { $size }
bench-progress = 進捗: { $percent }% ({ $current }/{ $total })
bench-results = ベンチマーク結果
bench-total-time = 総実行時間: { $duration }
bench-requests-sent = 送信リクエスト数: { $count }
bench-requests-success = 成功: { $count }
bench-requests-failed = 失敗: { $count }
bench-throughput = スループット: { $rate } req/s
bench-latency-avg = 平均レイテンシ: { $latency }
bench-latency-p50 = 50パーセンタイル: { $latency }
bench-latency-p95 = 95パーセンタイル: { $latency }
bench-latency-p99 = 99パーセンタイル: { $latency }
bench-bandwidth = 帯域幅: { $rate }
bench-error-rate = エラー率: { $rate }%

# エラーメッセージ
error-invalid-target = 無効なターゲットアドレス: { $target }
error-daemon-connection = デーモンへの接続に失敗: { $error }
error-network-error = ネットワークエラー: { $error }
error-timeout = 操作がタイムアウトしました
error-permission-denied = アクセスが拒否されました
error-invalid-stream-id = 無効なストリームID: { $stream_id }
error-stream-closed = ストリーム { $stream_id } は閉じられています
error-protocol-error = プロトコルエラー: { $error }

# 一般メッセージ
operation-cancelled = 操作がキャンセルされました
please-wait = お待ちください...
press-ctrl-c = Ctrl+Cでキャンセル
completed-successfully = 操作が正常に完了しました
warning = 警告: { $message }
info = 情報: { $message }

# テーブルヘッダー
header-error-code = エラーコード
header-description = 説明
header-count = 件数
header-stream-id = ストリームID
header-target = ターゲット
header-status = ステータス
header-duration = 期間
header-bytes = バイト数 