//! Simple i18n helper for Nyx CLI.
#![forbid(unsafe_code)]

use std::collections::HashMap;

static EN_MESSAGES: &[(&str, &str)] = &[
    ("connecting", "Connecting..."),
    ("connection_established", "Connection established"),
    ("daemon_version", "Daemon Version: {version}"),
    ("uptime", "Uptime: {uptime}"),
    ("network_bytes_in", "Bytes In: {bytes_in}"),
    ("network_bytes_out", "Bytes Out: {bytes_out}"),
    ("benchmark_target", "Target: {target}"),
    ("benchmark_duration", "Duration: {duration}s"),
    ("benchmark_connections", "Connections: {connections}"),
    ("benchmark_payload_size", "Payload Size: {payload_size} bytes"),
    ("benchmark_total_data", "Total Data: {total_data}"),
    ("benchmark_throughput", "Throughput: {throughput}"),
    ("benchmark_successful", "Successful Requests: {successful}"),
    ("benchmark_failed", "Failed Requests: {failed}"),
    ("benchmark_avg_latency", "Average Latency: {avg_latency}"),
    ("benchmark_p95_latency", "95th Percentile Latency: {p95_latency}"),
    ("benchmark_p99_latency", "99th Percentile Latency: {p99_latency}"),
];

static JA_MESSAGES: &[(&str, &str)] = &[
    ("connecting", "接続中..."),
    ("connection_established", "接続が確立されました"),
    ("daemon_version", "デーモンバージョン: {version}"),
    ("uptime", "稼働時間: {uptime}"),
    ("network_bytes_in", "受信バイト数: {bytes_in}"),
    ("network_bytes_out", "送信バイト数: {bytes_out}"),
    ("benchmark_target", "ターゲット: {target}"),
    ("benchmark_duration", "期間: {duration}秒"),
    ("benchmark_connections", "接続数: {connections}"),
    ("benchmark_payload_size", "ペイロードサイズ: {payload_size} バイト"),
    ("benchmark_total_data", "総データ量: {total_data}"),
    ("benchmark_throughput", "スループット: {throughput}"),
    ("benchmark_successful", "成功リクエスト数: {successful}"),
    ("benchmark_failed", "失敗リクエスト数: {failed}"),
    ("benchmark_avg_latency", "平均レイテンシ: {avg_latency}"),
    ("benchmark_p95_latency", "95パーセンタイルレイテンシ: {p95_latency}"),
    ("benchmark_p99_latency", "99パーセンタイルレイテンシ: {p99_latency}"),
];

static ZH_MESSAGES: &[(&str, &str)] = &[
    ("connecting", "连接中..."),
    ("connection_established", "连接已建立"),
    ("daemon_version", "守护进程版本: {version}"),
    ("uptime", "运行时间: {uptime}"),
    ("network_bytes_in", "接收字节数: {bytes_in}"),
    ("network_bytes_out", "发送字节数: {bytes_out}"),
    ("benchmark_target", "目标: {target}"),
    ("benchmark_duration", "持续时间: {duration}秒"),
    ("benchmark_connections", "连接数: {connections}"),
    ("benchmark_payload_size", "负载大小: {payload_size} 字节"),
    ("benchmark_total_data", "总数据量: {total_data}"),
    ("benchmark_throughput", "吞吐量: {throughput}"),
    ("benchmark_successful", "成功请求数: {successful}"),
    ("benchmark_failed", "失败请求数: {failed}"),
    ("benchmark_avg_latency", "平均延迟: {avg_latency}"),
    ("benchmark_p95_latency", "95百分位延迟: {p95_latency}"),
    ("benchmark_p99_latency", "99百分位延迟: {p99_latency}"),
];

fn get_messages(language: &str) -> HashMap<&'static str, &'static str> {
    let messages = match language {
        "ja" => JA_MESSAGES,
        "zh" => ZH_MESSAGES,
        _ => EN_MESSAGES,
    };
    messages.iter().cloned().collect()
}

pub fn localize(
    language: &str,
    text_id: &str,
    args: Option<&HashMap<&str, String>>,
) -> Result<String, Box<dyn std::error::Error>> {
    let messages = get_messages(language);
    
    let template = messages.get(text_id)
        .ok_or_else(|| format!("Message '{}' not found", text_id))?;
    
    let mut result = template.to_string();
    
    if let Some(args) = args {
        for (key, value) in args {
            let placeholder = format!("{{{}}}", key);
            result = result.replace(&placeholder, value);
        }
    }
    
    Ok(result)
} 