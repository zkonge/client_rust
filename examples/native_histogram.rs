use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::histogram::NativeHistogram;
use prometheus_client::registry::Registry;

/// Example demonstrating native histogram usage for measuring latencies.
///
/// Native histograms use exponential bucketing to automatically adapt to the
/// distribution of observed values, providing high resolution measurements
/// without the need to pre-configure bucket boundaries.
fn main() {
    let mut registry = Registry::default();

    // Create a native histogram with default settings (bucket factor 1.1)
    // This provides 8 buckets per power of two, which is a good balance
    // between precision and memory usage.
    let latency_histogram = NativeHistogram::new();

    registry.register(
        "http_request_latency",
        "HTTP request latency in seconds",
        latency_histogram.clone(),
    );

    // Simulate some HTTP requests with varying latencies
    println!("Simulating HTTP requests...\n");
    
    // Fast requests (< 10ms)
    for _ in 0..100 {
        let latency = 0.001 + rand::random::<f64>() * 0.009; // 1-10ms
        latency_histogram.observe(latency);
    }
    
    // Normal requests (10-100ms)
    for _ in 0..50 {
        let latency = 0.010 + rand::random::<f64>() * 0.090; // 10-100ms
        latency_histogram.observe(latency);
    }
    
    // Slow requests (100ms-1s)
    for _ in 0..10 {
        let latency = 0.100 + rand::random::<f64>() * 0.900; // 100ms-1s
        latency_histogram.observe(latency);
    }
    
    // Very slow requests (> 1s)
    for _ in 0..5 {
        let latency = 1.0 + rand::random::<f64>() * 5.0; // 1-6s
        latency_histogram.observe(latency);
    }

    // Encode and print the metrics
    let mut encoded = String::new();
    encode(&mut encoded, &registry).unwrap();

    println!("Exported metrics:\n{}", encoded);

    // Additional example: Higher precision histogram for more accurate measurements
    let mut high_precision_registry = Registry::default();
    
    // Create a native histogram with bucket factor 1.05 for ~16 buckets per power of two
    let precise_histogram = NativeHistogram::with_bucket_factor(1.05);
    
    high_precision_registry.register(
        "precise_measurements",
        "High precision measurements",
        precise_histogram.clone(),
    );

    // Observe some values
    for i in 0..20 {
        precise_histogram.observe((i as f64) * 0.1);
    }

    let mut precise_encoded = String::new();
    encode(&mut precise_encoded, &high_precision_registry).unwrap();
    
    println!("\nHigh precision histogram:\n{}", precise_encoded);
}
