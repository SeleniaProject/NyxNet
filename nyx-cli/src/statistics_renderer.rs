use anyhow::Result;
use chrono::{DateTime, Utc};
use comfy_table::{Table, presets::UTF8_FULL, Cell, Attribute, ContentArrangement};
use comfy_table::Color as TableColor;
use console::{style, Term};
use crossterm::{execute, terminal::{Clear, ClearType}, cursor::MoveTo};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::time::{Duration, Instant};

use crate::benchmark::{LayerMetrics, LayerPerformance};
use crate::latency_collector::{LatencyStatistics, LatencyPercentiles};
use crate::throughput_measurer::ThroughputStatistics;
use crate::error_tracker::ErrorStatistics;

/// Configuration for statistics display
#[derive(Debug, Clone)]
pub struct DisplayConfig {
    pub format: DisplayFormat,
    pub update_interval: Duration,
    pub show_layer_breakdown: bool,
    pub show_percentiles: bool,
    pub show_distribution: bool,
    pub show_time_series: bool,
    pub filter: StatisticsFilter,
}

/// Display format options
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayFormat {
    Table,
    Json,
    Summary,
    Compact,
}

/// Filter options for statistics display
#[derive(Debug, Clone)]
pub struct StatisticsFilter {
    pub time_range: Option<TimeRange>,
    pub connection_types: Vec<String>,
    pub stream_ids: Vec<String>,
    pub layer_filter: Option<String>,
    pub min_latency_ms: Option<f64>,
    pub max_latency_ms: Option<f64>,
}

/// Time range for filtering statistics
#[derive(Debug, Clone)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Comprehensive statistics data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsData {
    pub timestamp: DateTime<Utc>,
    pub summary: StatisticsSummary,
    pub latency_stats: LatencyStatistics,
    pub throughput_stats: ThroughputStatistics,
    pub error_stats: ErrorStatistics,
    pub layer_metrics: LayerMetrics,
    pub real_time_metrics: RealTimeMetrics,
}

/// High-level statistics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsSummary {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub throughput_mbps: f64,
    pub active_connections: u32,
    pub uptime_seconds: u64,
}

/// Real-time metrics for live display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealTimeMetrics {
    pub current_rps: f64,
    pub current_latency_ms: f64,
    pub current_throughput_mbps: f64,
    pub current_error_rate: f64,
    pub connection_health: ConnectionHealth,
    pub system_load: SystemLoad,
}

/// Connection health indicators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionHealth {
    pub healthy_connections: u32,
    pub degraded_connections: u32,
    pub failed_connections: u32,
    pub overall_health_score: f64, // 0.0-1.0
}

/// System load indicators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemLoad {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub network_utilization_percent: f64,
    pub daemon_health: String,
}

/// Main statistics renderer with multiple format support
pub struct StatisticsRenderer {
    config: DisplayConfig,
    term: Term,
    last_update: Instant,
    display_buffer: String,
}

impl StatisticsRenderer {
    /// Create a new statistics renderer with configuration
    pub fn new(config: DisplayConfig) -> Self {
        Self {
            config,
            term: Term::stdout(),
            last_update: Instant::now(),
            display_buffer: String::new(),
        }
    }

    /// Render statistics data in the configured format
    pub fn render(&mut self, data: &StatisticsData) -> Result<String> {
        match self.config.format {
            DisplayFormat::Table => self.render_table(data),
            DisplayFormat::Json => self.render_json(data),
            DisplayFormat::Summary => self.render_summary(data),
            DisplayFormat::Compact => self.render_compact(data),
        }
    }

    /// Display statistics with real-time updates and interactive controls
    pub async fn display_real_time(&mut self, data: &StatisticsData) -> Result<()> {
        if self.last_update.elapsed() < self.config.update_interval {
            return Ok(());
        }

        // Clear screen for real-time updates
        execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;

        let output = self.render(data)?;
        print!("{}", output);
        
        // Add interactive controls footer
        self.display_interactive_controls()?;
        
        io::stdout().flush()?;
        self.last_update = Instant::now();
        Ok(())
    }
    
    /// Display interactive control instructions
    fn display_interactive_controls(&self) -> Result<()> {
        println!("\n{}", style("Interactive Controls:").bold().blue());
        println!("{}", style("Press 'q' to quit, 'p' to pause, 'e' to export, 'f' to filter").dim());
        println!("{}", style("Use arrow keys to navigate, 's' to save snapshot").dim());
        Ok(())
    }
    
    /// Export statistics data to file
    pub fn export_to_file(&self, data: &StatisticsData, filename: &str, format: &str) -> Result<()> {
        use std::fs::File;
        use std::io::Write;
        
        let content = match format.to_lowercase().as_str() {
            "json" => serde_json::to_string_pretty(data)?,
            "csv" => self.export_to_csv(data)?,
            "txt" => self.render_table(data)?,
            _ => return Err(anyhow::anyhow!("Unsupported export format: {}", format)),
        };
        
        let mut file = File::create(filename)?;
        file.write_all(content.as_bytes())?;
        
        println!("{} Statistics exported to {}", style("✅").green(), filename);
        Ok(())
    }
    
    /// Export statistics to CSV format
    fn export_to_csv(&self, data: &StatisticsData) -> Result<String> {
        let mut csv = String::new();
        
        // CSV header
        csv.push_str("timestamp,total_requests,successful_requests,failed_requests,success_rate,");
        csv.push_str("avg_latency_ms,throughput_mbps,active_connections,uptime_seconds,");
        csv.push_str("current_rps,current_latency_ms,current_throughput_mbps,current_error_rate\n");
        
        // CSV data
        csv.push_str(&format!(
            "{},{},{},{},{:.2},{:.2},{:.2},{},{},{:.2},{:.2},{:.2},{:.2}\n",
            data.timestamp.format("%Y-%m-%d %H:%M:%S"),
            data.summary.total_requests,
            data.summary.successful_requests,
            data.summary.failed_requests,
            data.summary.success_rate,
            data.summary.avg_latency_ms,
            data.summary.throughput_mbps,
            data.summary.active_connections,
            data.summary.uptime_seconds,
            data.real_time_metrics.current_rps,
            data.real_time_metrics.current_latency_ms,
            data.real_time_metrics.current_throughput_mbps,
            data.real_time_metrics.current_error_rate
        ));
        
        Ok(csv)
    }
    
    /// Create a time series chart for latency trends
    pub fn create_latency_trend_chart(&self, data_points: &[(DateTime<Utc>, f64)]) -> Result<String> {
        let mut chart = String::new();
        
        if data_points.is_empty() {
            return Ok("No data available for trend chart".to_string());
        }
        
        chart.push_str(&format!("{}\n", style("Latency Trend (Last 60 seconds)").bold()));
        
        // Simple ASCII chart
        let max_latency = data_points.iter().map(|(_, latency)| *latency).fold(0.0, f64::max);
        let chart_height = 10;
        let chart_width = 60;
        
        for row in (0..chart_height).rev() {
            let threshold = (row as f64 / chart_height as f64) * max_latency;
            
            for col in 0..chart_width {
                if col < data_points.len() {
                    let (_, latency) = data_points[col];
                    if latency >= threshold {
                        chart.push('█');
                    } else {
                        chart.push(' ');
                    }
                } else {
                    chart.push(' ');
                }
            }
            chart.push_str(&format!(" {:.1}ms\n", threshold));
        }
        
        // X-axis labels
        chart.push_str(&"─".repeat(chart_width));
        chart.push_str(" Time\n");
        
        Ok(chart)
    }
    
    /// Apply filters to statistics data
    pub fn apply_filters(&self, data: &StatisticsData) -> StatisticsData {
        let mut filtered_data = data.clone();
        
        // Apply time range filter
        if let Some(time_range) = &self.config.filter.time_range {
            if data.timestamp < time_range.start || data.timestamp > time_range.end {
                // Return empty data if outside time range
                filtered_data.summary.total_requests = 0;
                filtered_data.summary.successful_requests = 0;
                filtered_data.summary.failed_requests = 0;
            }
        }
        
        // Apply latency filters
        if let Some(min_latency) = self.config.filter.min_latency_ms {
            if data.summary.avg_latency_ms < min_latency {
                filtered_data.summary.total_requests = 0;
            }
        }
        
        if let Some(max_latency) = self.config.filter.max_latency_ms {
            if data.summary.avg_latency_ms > max_latency {
                filtered_data.summary.total_requests = 0;
            }
        }
        
        filtered_data
    }

    /// Render statistics as a comprehensive table
    fn render_table(&self, data: &StatisticsData) -> Result<String> {
        let mut output = String::new();
        
        // Header with timestamp
        output.push_str(&format!(
            "{}\n{}\n\n",
            style("Nyx Network Statistics").bold().cyan(),
            style(format!("Updated: {}", data.timestamp.format("%Y-%m-%d %H:%M:%S UTC"))).dim()
        ));

        // Summary table
        output.push_str(&self.create_summary_table(&data.summary)?);
        output.push('\n');

        // Layer performance breakdown
        if self.config.show_layer_breakdown {
            output.push_str(&self.create_layer_table(&data.layer_metrics)?);
            output.push('\n');
        }

        // Latency percentiles
        if self.config.show_percentiles {
            output.push_str(&self.create_percentiles_table(&data.latency_stats.percentiles)?);
            output.push('\n');
        }

        // Real-time metrics
        output.push_str(&self.create_realtime_table(&data.real_time_metrics)?);
        output.push('\n');

        // Error breakdown
        if data.error_stats.total_errors > 0 {
            output.push_str(&self.create_error_table(&data.error_stats)?);
            output.push('\n');
        }

        // Distribution histogram
        if self.config.show_distribution && !data.latency_stats.distribution.buckets.is_empty() {
            output.push_str(&self.create_distribution_display(&data.latency_stats)?);
            output.push('\n');
        }

        Ok(output)
    }

    /// Create summary statistics table
    fn create_summary_table(&self, summary: &StatisticsSummary) -> Result<String> {
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Metric").add_attribute(Attribute::Bold),
            Cell::new("Value").add_attribute(Attribute::Bold),
            Cell::new("Status").add_attribute(Attribute::Bold),
        ]);

        // Total requests
        table.add_row(vec![
            Cell::new("Total Requests"),
            Cell::new(format!("{}", summary.total_requests)),
            Cell::new(self.get_status_indicator(summary.total_requests > 0)),
        ]);

        // Success rate with color coding
        let success_color = if summary.success_rate >= 99.0 {
            TableColor::Green
        } else if summary.success_rate >= 95.0 {
            TableColor::Yellow
        } else {
            TableColor::Red
        };
        
        table.add_row(vec![
            Cell::new("Success Rate"),
            Cell::new(format!("{:.2}%", summary.success_rate)).fg(success_color),
            Cell::new(self.get_performance_indicator(summary.success_rate, 95.0, 99.0)),
        ]);

        // Average latency with performance indicators
        let latency_color = if summary.avg_latency_ms <= 50.0 {
            TableColor::Green
        } else if summary.avg_latency_ms <= 100.0 {
            TableColor::Yellow
        } else {
            TableColor::Red
        };

        table.add_row(vec![
            Cell::new("Average Latency"),
            Cell::new(format!("{:.2}ms", summary.avg_latency_ms)).fg(latency_color),
            Cell::new(self.get_performance_indicator(100.0 - summary.avg_latency_ms, 50.0, 90.0)),
        ]);

        // Throughput
        let throughput_color = if summary.throughput_mbps >= 10.0 {
            TableColor::Green
        } else if summary.throughput_mbps >= 1.0 {
            TableColor::Yellow
        } else {
            TableColor::Red
        };

        table.add_row(vec![
            Cell::new("Throughput"),
            Cell::new(format!("{:.2} Mbps", summary.throughput_mbps)).fg(throughput_color),
            Cell::new(self.get_performance_indicator(summary.throughput_mbps, 1.0, 10.0)),
        ]);

        // Active connections
        table.add_row(vec![
            Cell::new("Active Connections"),
            Cell::new(format!("{}", summary.active_connections)),
            Cell::new(self.get_status_indicator(summary.active_connections > 0)),
        ]);

        // Uptime
        table.add_row(vec![
            Cell::new("Uptime"),
            Cell::new(self.format_duration(Duration::from_secs(summary.uptime_seconds))),
            Cell::new("✅"),
        ]);

        Ok(format!("{}\n{}", style("📊 Performance Summary").bold(), table))
    }

    /// Create layer performance breakdown table
    fn create_layer_table(&self, layer_metrics: &LayerMetrics) -> Result<String> {
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Layer").add_attribute(Attribute::Bold),
            Cell::new("Avg Latency").add_attribute(Attribute::Bold),
            Cell::new("Throughput").add_attribute(Attribute::Bold),
            Cell::new("Success Rate").add_attribute(Attribute::Bold),
            Cell::new("Errors").add_attribute(Attribute::Bold),
            Cell::new("Health").add_attribute(Attribute::Bold),
        ]);

        let layers = [
            ("Stream", &layer_metrics.stream_layer),
            ("Mix", &layer_metrics.mix_layer),
            ("FEC", &layer_metrics.fec_layer),
            ("Transport", &layer_metrics.transport_layer),
        ];

        for (name, layer) in &layers {
            let latency_color = if layer.latency_ms <= 25.0 {
                TableColor::Green
            } else if layer.latency_ms <= 50.0 {
                TableColor::Yellow
            } else {
                TableColor::Red
            };

            let success_color = if layer.success_rate >= 99.0 {
                TableColor::Green
            } else if layer.success_rate >= 95.0 {
                TableColor::Yellow
            } else {
                TableColor::Red
            };

            table.add_row(vec![
                Cell::new(*name),
                Cell::new(format!("{:.2}ms", layer.latency_ms)).fg(latency_color),
                Cell::new(format!("{:.2} Mbps", layer.throughput_mbps)),
                Cell::new(format!("{:.2}%", layer.success_rate)).fg(success_color),
                Cell::new(format!("{}", layer.error_count)),
                Cell::new(self.get_layer_health_indicator(layer)),
            ]);
        }

        Ok(format!("{}\n{}", style("🔧 Protocol Layer Performance").bold(), table))
    }

    /// Create latency percentiles table
    fn create_percentiles_table(&self, percentiles: &LatencyPercentiles) -> Result<String> {
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Percentile").add_attribute(Attribute::Bold),
            Cell::new("Latency").add_attribute(Attribute::Bold),
            Cell::new("Performance").add_attribute(Attribute::Bold),
        ]);

        let percentile_data = [
            ("50th (Median)", percentiles.p50),
            ("75th", percentiles.p75),
            ("90th", percentiles.p90),
            ("95th", percentiles.p95),
            ("99th", percentiles.p99),
            ("99.9th", percentiles.p99_9),
        ];

        for (name, value) in &percentile_data {
            let color = if *value <= 50.0 {
                TableColor::Green
            } else if *value <= 100.0 {
                TableColor::Yellow
            } else {
                TableColor::Red
            };

            table.add_row(vec![
                Cell::new(*name),
                Cell::new(format!("{:.2}ms", value)).fg(color),
                Cell::new(self.get_latency_performance_bar(*value)),
            ]);
        }

        Ok(format!("{}\n{}", style("📈 Latency Distribution").bold(), table))
    }

    /// Create real-time metrics table
    fn create_realtime_table(&self, metrics: &RealTimeMetrics) -> Result<String> {
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Real-time Metric").add_attribute(Attribute::Bold),
            Cell::new("Current Value").add_attribute(Attribute::Bold),
            Cell::new("Trend").add_attribute(Attribute::Bold),
        ]);

        // Current RPS
        table.add_row(vec![
            Cell::new("Requests/sec"),
            Cell::new(format!("{:.1}", metrics.current_rps)),
            Cell::new("📊"), // Could be enhanced with actual trend data
        ]);

        // Current latency
        let latency_color = if metrics.current_latency_ms <= 50.0 {
            TableColor::Green
        } else if metrics.current_latency_ms <= 100.0 {
            TableColor::Yellow
        } else {
            TableColor::Red
        };

        table.add_row(vec![
            Cell::new("Current Latency"),
            Cell::new(format!("{:.2}ms", metrics.current_latency_ms)).fg(latency_color),
            Cell::new(self.get_trend_indicator(metrics.current_latency_ms, 75.0)),
        ]);

        // Current throughput
        table.add_row(vec![
            Cell::new("Current Throughput"),
            Cell::new(format!("{:.2} Mbps", metrics.current_throughput_mbps)),
            Cell::new("📈"),
        ]);

        // Error rate
        let error_color = if metrics.current_error_rate <= 1.0 {
            TableColor::Green
        } else if metrics.current_error_rate <= 5.0 {
            TableColor::Yellow
        } else {
            TableColor::Red
        };

        table.add_row(vec![
            Cell::new("Error Rate"),
            Cell::new(format!("{:.2}%", metrics.current_error_rate)).fg(error_color),
            Cell::new(self.get_trend_indicator(10.0 - metrics.current_error_rate, 5.0)),
        ]);

        // Connection health
        table.add_row(vec![
            Cell::new("Connection Health"),
            Cell::new(format!("{:.1}%", metrics.connection_health.overall_health_score * 100.0)),
            Cell::new(self.get_health_indicator(metrics.connection_health.overall_health_score)),
        ]);

        Ok(format!("{}\n{}", style("⚡ Real-time Metrics").bold(), table))
    }

    /// Create error statistics table
    fn create_error_table(&self, error_stats: &ErrorStatistics) -> Result<String> {
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec![
            Cell::new("Error Category").add_attribute(Attribute::Bold),
            Cell::new("Count").add_attribute(Attribute::Bold),
            Cell::new("Percentage").add_attribute(Attribute::Bold),
            Cell::new("Trend").add_attribute(Attribute::Bold),
        ]);

        for (category, error_type_stats) in &error_stats.error_rate_by_type {
            let count = error_type_stats.count;
            let percentage = error_type_stats.percentage;

            let color = if percentage <= 5.0 {
                TableColor::Green
            } else if percentage <= 15.0 {
                TableColor::Yellow
            } else {
                TableColor::Red
            };

            table.add_row(vec![
                Cell::new(category),
                Cell::new(format!("{}", count)).fg(color),
                Cell::new(format!("{:.1}%", percentage)).fg(color),
                Cell::new("📊"), // Could be enhanced with trend analysis
            ]);
        }

        Ok(format!("{}\n{}", style("❌ Error Analysis").bold(), table))
    }

    /// Create latency distribution histogram display
    fn create_distribution_display(&self, latency_stats: &LatencyStatistics) -> Result<String> {
        let mut output = String::new();
        output.push_str(&format!("{}\n", style("📊 Latency Distribution Histogram").bold()));

        if latency_stats.distribution.buckets.is_empty() {
            output.push_str("No distribution data available\n");
            return Ok(output);
        }

        let max_count = latency_stats.distribution.buckets
            .iter()
            .map(|b| b.count)
            .max()
            .unwrap_or(1);

        for bucket in &latency_stats.distribution.buckets {
            if bucket.count > 0 {
                let bar_length = (bucket.count * 40) / max_count.max(1);
                let bar = "█".repeat(bar_length);
                let color = if bucket.range_start_ms <= 50.0 {
                    console::Color::Green
                } else if bucket.range_start_ms <= 100.0 {
                    console::Color::Yellow
                } else {
                    console::Color::Red
                };

                output.push_str(&format!(
                    "{:6.1}-{:6.1}ms: {:>4} ({:4.1}%) {}\n",
                    bucket.range_start_ms,
                    bucket.range_end_ms,
                    bucket.count,
                    bucket.percentage,
                    style(bar).fg(color)
                ));
            }
        }

        Ok(output)
    }

    /// Render statistics as JSON
    fn render_json(&self, data: &StatisticsData) -> Result<String> {
        Ok(serde_json::to_string_pretty(data)?)
    }

    /// Render statistics as a compact summary
    fn render_summary(&self, data: &StatisticsData) -> Result<String> {
        Ok(format!(
            "Nyx Statistics Summary ({})\n\
             Requests: {} (Success: {:.1}%) | Latency: {:.1}ms | Throughput: {:.1} Mbps | Errors: {}\n\
             Connections: {} active | Health: {:.1}% | Uptime: {}\n",
            data.timestamp.format("%H:%M:%S"),
            data.summary.total_requests,
            data.summary.success_rate,
            data.summary.avg_latency_ms,
            data.summary.throughput_mbps,
            data.error_stats.total_errors,
            data.summary.active_connections,
            data.real_time_metrics.connection_health.overall_health_score * 100.0,
            self.format_duration(Duration::from_secs(data.summary.uptime_seconds))
        ))
    }

    /// Render statistics in compact format
    fn render_compact(&self, data: &StatisticsData) -> Result<String> {
        Ok(format!(
            "{} | RPS: {:.0} | Lat: {:.0}ms | Thr: {:.1}Mbps | Err: {:.1}% | Conn: {} | Health: {:.0}%",
            data.timestamp.format("%H:%M:%S"),
            data.real_time_metrics.current_rps,
            data.real_time_metrics.current_latency_ms,
            data.real_time_metrics.current_throughput_mbps,
            data.real_time_metrics.current_error_rate,
            data.summary.active_connections,
            data.real_time_metrics.connection_health.overall_health_score * 100.0
        ))
    }

    // Helper methods for formatting and indicators

    fn get_status_indicator(&self, is_good: bool) -> &'static str {
        if is_good { "✅" } else { "❌" }
    }

    fn get_performance_indicator(&self, value: f64, warning_threshold: f64, good_threshold: f64) -> &'static str {
        if value >= good_threshold {
            "🟢"
        } else if value >= warning_threshold {
            "🟡"
        } else {
            "🔴"
        }
    }

    fn get_layer_health_indicator(&self, layer: &LayerPerformance) -> &'static str {
        if layer.success_rate >= 99.0 && layer.latency_ms <= 50.0 {
            "🟢"
        } else if layer.success_rate >= 95.0 && layer.latency_ms <= 100.0 {
            "🟡"
        } else {
            "🔴"
        }
    }

    fn get_latency_performance_bar(&self, latency_ms: f64) -> String {
        let bar_length = if latency_ms <= 25.0 {
            8
        } else if latency_ms <= 50.0 {
            6
        } else if latency_ms <= 100.0 {
            4
        } else {
            2
        };
        
        let color = if latency_ms <= 50.0 {
            console::Color::Green
        } else if latency_ms <= 100.0 {
            console::Color::Yellow
        } else {
            console::Color::Red
        };

        format!("{}", style("█".repeat(bar_length)).fg(color))
    }

    fn get_trend_indicator(&self, value: f64, threshold: f64) -> &'static str {
        if value >= threshold {
            "📈"
        } else {
            "📉"
        }
    }

    fn get_health_indicator(&self, health_score: f64) -> &'static str {
        if health_score >= 0.9 {
            "💚"
        } else if health_score >= 0.7 {
            "💛"
        } else {
            "❤️"
        }
    }

    fn format_duration(&self, duration: Duration) -> String {
        let total_seconds = duration.as_secs();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;

        if hours > 0 {
            format!("{}h {}m {}s", hours, minutes, seconds)
        } else if minutes > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}s", seconds)
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            format: DisplayFormat::Table,
            update_interval: Duration::from_secs(5),
            show_layer_breakdown: true,
            show_percentiles: true,
            show_distribution: false,
            show_time_series: false,
            filter: StatisticsFilter::default(),
        }
    }
}

impl Default for StatisticsFilter {
    fn default() -> Self {
        Self {
            time_range: None,
            connection_types: Vec::new(),
            stream_ids: Vec::new(),
            layer_filter: None,
            min_latency_ms: None,
            max_latency_ms: None,
        }
    }
}