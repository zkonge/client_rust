# Prometheus Rust client library

[![Test Status](https://github.com/prometheus/client_rust/actions/workflows/rust.yml/badge.svg?event=push)](https://github.com/prometheus/client_rust/actions)
[![Crate](https://img.shields.io/crates/v/prometheus-client.svg)](https://crates.io/crates/prometheus-client)
[![API](https://docs.rs/prometheus-client/badge.svg)](https://docs.rs/prometheus-client)

[Rust](https://github.com/rust-lang/) client library implementation of the [Open
Metrics specification](https://github.com/OpenObservability/OpenMetrics). Allows
developers to instrument applications and thus enables operators to monitor said
applications with monitoring systems like [Prometheus](https://prometheus.io/).

**Documentation**: https://docs.rs/prometheus-client/

## Goals

- No `unsafe`. Don't use unsafe Rust within the library itself.

- Type safe. Leverage Rust's type system to catch common instrumentation
  mistakes at compile time.

- Fast. Don't force users to worry about the performance impact of
  instrumentation. Instead encourage users to instrument often and extensively.

## Features

### Native Histograms

This library supports [Prometheus Native Histograms](https://prometheus.io/docs/concepts/metric_types/#histogram), 
which provide high-resolution measurement of distributions using exponential bucketing. 
Unlike traditional histograms with fixed bucket boundaries, native histograms automatically 
adapt to the distribution of observed values.

```rust
use prometheus_client::metrics::histogram::NativeHistogram;
use prometheus_client::registry::Registry;

let mut registry = Registry::default();

// Create a native histogram with default settings (bucket factor 1.1)
let latency = NativeHistogram::new();
registry.register("request_latency", "Request latency in seconds", latency.clone());

// Observe some values
latency.observe(0.005);  // 5ms
latency.observe(0.150);  // 150ms
latency.observe(2.500);  // 2.5s
```

Key benefits:
- **Automatic bucketing**: No need to pre-configure bucket boundaries
- **High resolution**: Configurable precision via bucket factor
- **Efficient storage**: Sparse representation only stores non-empty buckets
- **Wide range**: Handles values from near-zero to very large automatically

See the [native_histogram example](examples/native_histogram.rs) for more details.

## Specification Compliance

Below is a list of properties where this client library implementation lags
behind the Open Metrics specification. Not being compliant with all requirements
(`MUST` and `MUST NOT`) of the specification is considered a bug and likely to
be fixed in the future. Contributions in all forms are most welcome.

- State set metric.

- Enforce "A Histogram MetricPoint MUST contain at least one bucket".

- Enforce "A MetricFamily MUST have a [...] UNIT metadata".

- Enforce "MetricFamily names [...] MUST be unique within a MetricSet."

- Enforce "Names SHOULD be in snake_case".

- Enforce "MetricFamily names beginning with underscores are RESERVED and MUST
  NOT be used unless specified by this standard".

- Enforce "Exposers SHOULD avoid names that could be confused with the suffixes
  that text format sample metric names use".

- Gauge histogram metric.

- Allow "A MetricPoint in a Metric with the type [Counter, Histogram] SHOULD have a Timestamp
  value called Created".

- Summary metric.

## Related Libraries

- [rust-prometheus](https://github.com/tikv/rust-prometheus/): See [tikv/rust-prometheus/#392](https://github.com/tikv/rust-prometheus/issues/392) for a high-level comparison.

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

#### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
