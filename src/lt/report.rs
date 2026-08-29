use super::types::LtResult;

/// Print the final load-test report.
pub fn print_results(result: &LtResult, url: &str) {
    use hdrhistogram::Histogram;
    use std::collections::HashMap;

    if result.responses.is_empty() {
        println!("No responses were recorded.");
        if !result.errors.is_empty() {
            println!("\nErrors:");
            for err in &result.errors {
                println!("  [{}] {}", err.count, err.message);
            }
        }
        return;
    }

    let histogram_res = Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3);
    let Ok(mut histogram) = histogram_res else {
        println!("histogram_res is error");
        return;
    };
    for resp in &result.responses {
        histogram.record(resp.time_cost_ns).unwrap_or_default();
    }

    let duration_secs = result.duration_ns as f64 / 1_000_000_000.0;
    let total_requests = result.responses.len() as u64;

    // Calculate status codes
    let mut status_codes: HashMap<u16, u64> = HashMap::new();
    for resp in &result.responses {
        *status_codes.entry(resp.status_code).or_insert(0) += 1;
    }

    // Helper function to format latency
    fn format_latency_ns(ns: u64) -> String {
        if ns >= 1_000_000_000 {
            format!("{:.2} s", ns as f64 / 1_000_000_000.0)
        } else if ns >= 1_000_000 {
            format!("{:.2} ms", ns as f64 / 1_000_000.0)
        } else if ns >= 1_000 {
            format!("{:.2} µs", ns as f64 / 1_000.0)
        } else {
            format!("{} ns", ns)
        }
    }

    // Helper function to get HTTP status description
    fn status_description(code: u16) -> &'static str {
        match code {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            301 => "Moved Permanently",
            302 => "Found",
            304 => "Not Modified",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            _ => "Unknown",
        }
    }

    println!("\n=== Load Test Results ===");
    println!("URL: {}", url);
    println!("Duration: {:.2}s", duration_secs);
    println!("Total Requests: {}", total_requests);
    println!("Requests/sec: {:.2}", total_requests as f64 / duration_secs);
    println!(
        "Transfer: {:.2} MB",
        result.total_bytes as f64 / (1024.0 * 1024.0)
    );
    println!("\nLatency:");
    println!("  Average: {}", format_latency_ns(histogram.mean() as u64));
    println!("  Min:     {}", format_latency_ns(histogram.min()));
    println!("  Max:     {}", format_latency_ns(histogram.max()));
    println!(
        "  P50:     {}",
        format_latency_ns(histogram.value_at_quantile(0.5))
    );
    println!(
        "  P90:     {}",
        format_latency_ns(histogram.value_at_quantile(0.9))
    );
    println!(
        "  P95:     {}",
        format_latency_ns(histogram.value_at_quantile(0.95))
    );
    println!(
        "  P99:     {}",
        format_latency_ns(histogram.value_at_quantile(0.99))
    );

    println!("\nStatus Codes:");
    let mut codes: Vec<_> = status_codes.iter().collect();
    codes.sort_by_key(|&(k, _)| k);
    for (code, count) in codes {
        let percent = (*count as f64 / total_requests as f64) * 100.0;
        println!(
            "  {} {} - {} ({:.1}%)",
            code,
            status_description(*code),
            count,
            percent
        );
    }

    if !result.errors.is_empty() {
        println!("\nErrors:");
        for err in &result.errors {
            println!("  [{}] {}", err.count, err.message);
        }
    }
}
