//! Module implementing an Open Metrics histogram.
//!
//! See [`Histogram`] for details.

use crate::encoding::{EncodeMetric, MetricEncoder, NoLabelSet};

use super::{MetricType, TypedMetric};
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};
use std::collections::BTreeMap;
use std::iter::{self, once};
use std::sync::Arc;

/// Open Metrics [`Histogram`] to measure distributions of discrete events.
///
/// ```
/// # use prometheus_client::metrics::histogram::{Histogram, exponential_buckets};
/// let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
/// histogram.observe(4.2);
/// ```
///
/// [`Histogram`] does not implement [`Default`], given that the choice of
/// bucket values depends on the situation [`Histogram`] is used in. As an
/// example, to measure HTTP request latency, the values suggested in the
/// Golang implementation might work for you:
///
/// ```
/// # use prometheus_client::metrics::histogram::Histogram;
/// // Default values from go client(https://github.com/prometheus/client_golang/blob/5d584e2717ef525673736d72cd1d12e304f243d7/prometheus/histogram.go#L68)
/// let custom_buckets = [
///    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
/// ];
/// let histogram = Histogram::new(custom_buckets);
/// histogram.observe(4.2);
/// ```
// TODO: Consider using atomics. See
// https://github.com/tikv/rust-prometheus/pull/314.
#[derive(Debug)]
pub struct Histogram {
    inner: Arc<RwLock<Inner>>,
}

impl Clone for Histogram {
    fn clone(&self) -> Self {
        Histogram {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Inner {
    // TODO: Consider allowing integer observe values.
    sum: f64,
    count: u64,
    // TODO: Consider being generic over the bucket length.
    buckets: Vec<(f64, u64)>,
}

impl Histogram {
    /// Create a new [`Histogram`].
    ///
    /// ```rust
    /// # use prometheus_client::metrics::histogram::Histogram;
    /// let histogram = Histogram::new([10.0, 100.0, 1_000.0]);
    /// ```
    pub fn new(buckets: impl IntoIterator<Item = f64>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                sum: Default::default(),
                count: Default::default(),
                buckets: buckets
                    .into_iter()
                    .chain(once(f64::MAX))
                    .map(|upper_bound| (upper_bound, 0))
                    .collect(),
            })),
        }
    }

    /// Observe the given value.
    pub fn observe(&self, v: f64) {
        self.observe_and_bucket(v);
    }

    /// Observes the given value, returning the index of the first bucket the
    /// value is added to.
    ///
    /// Needed in
    /// [`HistogramWithExemplars`](crate::metrics::exemplar::HistogramWithExemplars).
    pub(crate) fn observe_and_bucket(&self, v: f64) -> Option<usize> {
        let mut inner = self.inner.write();
        inner.sum += v;
        inner.count += 1;

        let first_bucket = inner
            .buckets
            .iter_mut()
            .enumerate()
            .find(|(_i, (upper_bound, _value))| upper_bound >= &v);

        match first_bucket {
            Some((i, (_upper_bound, value))) => {
                *value += 1;
                Some(i)
            }
            None => None,
        }
    }

    pub(crate) fn get(&self) -> (f64, u64, MappedRwLockReadGuard<'_, Vec<(f64, u64)>>) {
        let inner = self.inner.read();
        let sum = inner.sum;
        let count = inner.count;
        let buckets = RwLockReadGuard::map(inner, |inner| &inner.buckets);
        (sum, count, buckets)
    }
}

impl TypedMetric for Histogram {
    const TYPE: MetricType = MetricType::Histogram;
}

/// Exponential bucket distribution.
pub fn exponential_buckets(start: f64, factor: f64, length: u16) -> impl Iterator<Item = f64> {
    iter::repeat(())
        .enumerate()
        .map(move |(i, _)| start * factor.powf(i as f64))
        .take(length.into())
}

/// Exponential bucket distribution within a range
///
/// Creates `length` buckets, where the lowest bucket is `min` and the highest bucket is `max`.
///
/// If `length` is less than 1, or `min` is less than or equal to 0, an empty iterator is returned.
pub fn exponential_buckets_range(min: f64, max: f64, length: u16) -> impl Iterator<Item = f64> {
    let mut len_observed = length;
    let mut min_bucket = min;
    // length needs a positive length and min needs to be greater than 0
    // set len_observed to 0 and min_bucket to 1.0
    // this will return an empty iterator in the result
    if length < 1 || min <= 0.0 {
        len_observed = 0;
        min_bucket = 1.0;
    }
    // We know max/min and highest bucket. Solve for growth_factor.
    let growth_factor = (max / min_bucket).powf(1.0 / (len_observed as f64 - 1.0));

    iter::repeat(())
        .enumerate()
        .map(move |(i, _)| min_bucket * growth_factor.powf(i as f64))
        .take(len_observed.into())
}

/// Linear bucket distribution.
pub fn linear_buckets(start: f64, width: f64, length: u16) -> impl Iterator<Item = f64> {
    iter::repeat(())
        .enumerate()
        .map(move |(i, _)| start + (width * (i as f64)))
        .take(length.into())
}

impl EncodeMetric for Histogram {
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), std::fmt::Error> {
        let (sum, count, buckets) = self.get();
        encoder.encode_histogram::<NoLabelSet>(sum, count, &buckets, None)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram() {
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        histogram.observe(1.0);
    }

    #[test]
    fn exponential() {
        assert_eq!(
            vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0],
            exponential_buckets(1.0, 2.0, 10).collect::<Vec<_>>()
        );
    }

    #[test]
    fn linear() {
        assert_eq!(
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
            linear_buckets(0.0, 1.0, 10).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exponential_range() {
        assert_eq!(
            vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0],
            exponential_buckets_range(1.0, 32.0, 6).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exponential_range_incorrect() {
        let res = exponential_buckets_range(1.0, 32.0, 0).collect::<Vec<_>>();
        assert!(res.is_empty());

        let res = exponential_buckets_range(0.0, 32.0, 6).collect::<Vec<_>>();
        assert!(res.is_empty());
    }
}

/// Default zero threshold for native histograms.
///
/// The value is 2^-128 (or 0.5*2^-127 in the actual IEEE 754 representation),
/// which is a bucket boundary at all possible resolutions.
pub const DEFAULT_NATIVE_HISTOGRAM_ZERO_THRESHOLD: f64 = 2.938735877055719e-39;

/// Native histogram schema limits.
const NATIVE_HISTOGRAM_SCHEMA_MAX: i32 = 8;
const NATIVE_HISTOGRAM_SCHEMA_MIN: i32 = -4;

/// Open Metrics native [`Histogram`] to measure distributions using exponential bucketing.
///
/// Native histograms use sparse exponential bucketing to provide high resolution
/// measurements with efficient storage. Unlike traditional histograms with fixed
/// buckets, native histograms automatically adjust their bucket boundaries based
/// on observed values.
///
/// ```
/// # use prometheus_client::metrics::histogram::NativeHistogram;
/// let histogram = NativeHistogram::new();
/// histogram.observe(4.2);
/// histogram.observe(0.001);
/// histogram.observe(1000.0);
/// ```
///
/// The bucket factor determines the resolution - smaller values provide more buckets
/// and higher accuracy at the cost of memory. A factor of 1.1 (default) provides a
/// good balance, dividing each power of two into 8 buckets.
#[derive(Debug)]
pub struct NativeHistogram {
    inner: Arc<RwLock<NativeInner>>,
}

impl Clone for NativeHistogram {
    fn clone(&self) -> Self {
        NativeHistogram {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug)]
struct NativeInner {
    sum: f64,
    count: u64,
    zero_count: u64,
    zero_threshold: f64,
    schema: i32,
    positive_buckets: BTreeMap<i32, u64>,
    negative_buckets: BTreeMap<i32, u64>,
}

impl NativeHistogram {
    /// Create a new [`NativeHistogram`] with default settings.
    ///
    /// Uses a bucket factor of 1.1 which provides 8 buckets per power of two.
    ///
    /// ```rust
    /// # use prometheus_client::metrics::histogram::NativeHistogram;
    /// let histogram = NativeHistogram::new();
    /// ```
    pub fn new() -> Self {
        Self::with_bucket_factor(1.1)
    }

    /// Create a new [`NativeHistogram`] with a specific bucket factor.
    ///
    /// The bucket factor determines the resolution of the histogram. Each bucket's
    /// upper bound is at most `bucket_factor` times larger than the previous bucket.
    /// Smaller factors provide more precision but use more memory.
    ///
    /// Common values:
    /// - 1.1 (default): 8 buckets per power of two
    /// - 1.05: ~16 buckets per power of two  
    /// - 1.2: ~4 buckets per power of two
    ///
    /// ```rust
    /// # use prometheus_client::metrics::histogram::NativeHistogram;
    /// // Higher precision with more buckets
    /// let histogram = NativeHistogram::with_bucket_factor(1.05);
    /// ```
    pub fn with_bucket_factor(bucket_factor: f64) -> Self {
        let schema = pick_schema(bucket_factor);
        Self {
            inner: Arc::new(RwLock::new(NativeInner {
                sum: 0.0,
                count: 0,
                zero_count: 0,
                zero_threshold: DEFAULT_NATIVE_HISTOGRAM_ZERO_THRESHOLD,
                schema,
                positive_buckets: BTreeMap::new(),
                negative_buckets: BTreeMap::new(),
            })),
        }
    }

    /// Create a new [`NativeHistogram`] with a custom zero threshold.
    ///
    /// Values with absolute value less than or equal to the zero threshold
    /// are accumulated in a special "zero" bucket.
    ///
    /// ```rust
    /// # use prometheus_client::metrics::histogram::NativeHistogram;
    /// // Use a zero threshold of 0.001
    /// let histogram = NativeHistogram::with_zero_threshold(1.1, 0.001);
    /// ```
    pub fn with_zero_threshold(bucket_factor: f64, zero_threshold: f64) -> Self {
        let schema = pick_schema(bucket_factor);
        Self {
            inner: Arc::new(RwLock::new(NativeInner {
                sum: 0.0,
                count: 0,
                zero_count: 0,
                zero_threshold,
                schema,
                positive_buckets: BTreeMap::new(),
                negative_buckets: BTreeMap::new(),
            })),
        }
    }

    /// Observe the given value.
    ///
    /// Records the value in the appropriate exponential bucket. Positive and
    /// negative values are tracked separately. Values within the zero threshold
    /// are accumulated in the zero bucket.
    pub fn observe(&self, value: f64) {
        let mut inner = self.inner.write();
        inner.sum += value;
        inner.count += 1;

        // Handle NaN - just add to count and sum
        if value.is_nan() {
            return;
        }

        let abs_value = value.abs();

        // Check if value falls in zero bucket
        if abs_value <= inner.zero_threshold {
            inner.zero_count += 1;
            return;
        }

        // Calculate bucket index
        let bucket_index = calculate_bucket_index(abs_value, inner.schema);

        // Add to appropriate bucket map
        if value > 0.0 {
            *inner.positive_buckets.entry(bucket_index).or_insert(0) += 1;
        } else {
            *inner.negative_buckets.entry(bucket_index).or_insert(0) += 1;
        }
    }

    /// Get the current state of the histogram for encoding.
    pub(crate) fn get(
        &self,
    ) -> (
        f64,
        u64,
        u64,
        f64,
        i32,
        Vec<(i32, u64)>,
        Vec<(i32, u64)>,
    ) {
        let inner = self.inner.read();
        let sum = inner.sum;
        let count = inner.count;
        let zero_count = inner.zero_count;
        let zero_threshold = inner.zero_threshold;
        let schema = inner.schema;
        let positive_buckets: Vec<(i32, u64)> = inner.positive_buckets.iter().map(|(&k, &v)| (k, v)).collect();
        let negative_buckets: Vec<(i32, u64)> = inner.negative_buckets.iter().map(|(&k, &v)| (k, v)).collect();
        (sum, count, zero_count, zero_threshold, schema, positive_buckets, negative_buckets)
    }
}

impl Default for NativeHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl TypedMetric for NativeHistogram {
    const TYPE: MetricType = MetricType::Histogram;
}

impl EncodeMetric for NativeHistogram {
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), std::fmt::Error> {
        let (sum, count, zero_count, zero_threshold, schema, positive_buckets, negative_buckets) = self.get();
        encoder.encode_native_histogram::<NoLabelSet>(
            sum,
            count,
            zero_count,
            zero_threshold,
            schema,
            &positive_buckets,
            &negative_buckets,
        )
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

/// Pick the appropriate schema for a given bucket factor.
///
/// Returns the largest schema number n between -4 and 8 such that
/// 2^(2^-n) is less than or equal to the provided bucket_factor.
fn pick_schema(bucket_factor: f64) -> i32 {
    if bucket_factor <= 1.0 {
        panic!("bucket_factor must be greater than 1.0");
    }

    // Calculate log2(log2(bucket_factor))
    // log2(x) = ln(x) / ln(2), so log2(log2(x)) = ln(ln(x)/ln(2)) / ln(2)
    let ln2 = 2.0_f64.ln();
    let floor = ((bucket_factor.ln() / ln2).ln() / ln2).floor() as i32;

    match floor {
        i32::MIN..=-8 => NATIVE_HISTOGRAM_SCHEMA_MAX,
        4..=i32::MAX => NATIVE_HISTOGRAM_SCHEMA_MIN,
        _ => -floor,
    }
}

/// Calculate the bucket index for a given value and schema.
///
/// The bucket index represents which exponential bucket the value falls into.
/// Index 0 corresponds to values in the range [1, base) where base = 2^(2^-schema).
fn calculate_bucket_index(value: f64, schema: i32) -> i32 {
    if value <= 0.0 || value.is_nan() || value.is_infinite() {
        // Handle edge cases
        if value.is_infinite() && value.is_sign_positive() {
            return i32::MAX;
        } else if value.is_infinite() {
            return i32::MIN;
        }
        return 0;
    }

    // Get the fractional part (0.5 <= frac < 1.0) and exponent
    let (frac, exp) = frexp(value);

    if schema > 0 {
        // For positive schema, use binary search in precomputed bounds
        let bounds = get_native_histogram_bounds(schema);
        let frac_index = bounds.iter().position(|&b| frac < b).unwrap_or(bounds.len());
        frac_index as i32 + (exp - 1) * bounds.len() as i32
    } else {
        // For non-positive schema, use a simpler calculation
        let mut key = exp;
        if frac == 0.5 {
            key -= 1;
        }
        let offset = (1 << -schema) - 1;
        (key + offset) >> -schema
    }
}

/// Extract mantissa and exponent from a float (similar to C's frexp).
fn frexp(value: f64) -> (f64, i32) {
    if value == 0.0 {
        return (0.0, 0);
    }
    let bits = value.to_bits();
    let exp_bits = ((bits >> 52) & 0x7FF) as i32;
    
    if exp_bits == 0 {
        // Subnormal number
        let normalized = value * 2.0_f64.powi(1022);
        let (frac, exp) = frexp(normalized);
        return (frac, exp - 1022);
    }
    
    let exp = exp_bits - 1022; // 1023 (bias) - 1 (for 0.5 <= frac < 1.0)
    let frac_bits = (bits & 0xFFFFFFFFFFFFF) | 0x3FE0000000000000; // Set exponent to -1
    let frac = f64::from_bits(frac_bits);
    
    (frac, exp)
}

/// Get precomputed bucket boundaries for a given schema.
fn get_native_histogram_bounds(schema: i32) -> &'static [f64] {
    match schema {
        0 => &[0.5],
        1 => &[0.5, 0.7071067811865475],
        2 => &[0.5, 0.5946035575013605, 0.7071067811865475, 0.8408964152537144],
        3 => &[
            0.5, 0.5452538663326288, 0.5946035575013605, 0.6484197773255048,
            0.7071067811865475, 0.7711054127039704, 0.8408964152537144, 0.9170040432046711,
        ],
        4 => &[
            0.5, 0.5221368912137069, 0.5452538663326288, 0.5693943173783458,
            0.5946035575013605, 0.620928906036742, 0.6484197773255048, 0.6771277734684463,
            0.7071067811865475, 0.7384130729697496, 0.7711054127039704, 0.805245165974627,
            0.8408964152537144, 0.8781260801866495, 0.9170040432046711, 0.9576032806985735,
        ],
        5 => &[
            0.5, 0.5109485743270583, 0.5221368912137069, 0.5335702003384117,
            0.5452538663326288, 0.5571933712979462, 0.5693943173783458, 0.5818624293887887,
            0.5946035575013605, 0.6076236799902344, 0.620928906036742, 0.6345254785958666,
            0.6484197773255048, 0.6626183215798706, 0.6771277734684463, 0.6919549409819159,
            0.7071067811865475, 0.7225904034885232, 0.7384130729697496, 0.7545822137967112,
            0.7711054127039704, 0.7879904225539431, 0.805245165974627, 0.8228777390769823,
            0.8408964152537144, 0.8593096490612387, 0.8781260801866495, 0.8973545375015533,
            0.9170040432046711, 0.9370838170551498, 0.9576032806985735, 0.9785720620876999,
        ],
        6 => &[
            0.5, 0.5054446430258502, 0.5109485743270583, 0.5165124395106142,
            0.5221368912137069, 0.5278225891802786, 0.5335702003384117, 0.5393803988785598,
            0.5452538663326288, 0.5511912916539204, 0.5571933712979462, 0.5632608093041209,
            0.5693943173783458, 0.5755946149764913, 0.5818624293887887, 0.5881984958251406,
            0.5946035575013605, 0.6010783657263515, 0.6076236799902344, 0.6142402680534349,
            0.620928906036742, 0.6276903785123455, 0.6345254785958666, 0.6414350080393891,
            0.6484197773255048, 0.6554806057623822, 0.6626183215798706, 0.6698337620266515,
            0.6771277734684463, 0.6845012114872953, 0.6919549409819159, 0.6994898362691555,
            0.7071067811865475, 0.7148066691959849, 0.7225904034885232, 0.7304588970903234,
            0.7384130729697496, 0.7464538641456323, 0.7545822137967112, 0.762799075372269,
            0.7711054127039704, 0.7795022001189185, 0.7879904225539431, 0.7965710756711334,
            0.805245165974627, 0.8140137109286738, 0.8228777390769823, 0.8318382901633681,
            0.8408964152537144, 0.8500531768592616, 0.8593096490612387, 0.8686669176368529,
            0.8781260801866495, 0.8876882462632604, 0.8973545375015533, 0.9071260877501991,
            0.9170040432046711, 0.9269895625416926, 0.9370838170551498, 0.9472879907934827,
            0.9576032806985735, 0.9680308967461471, 0.9785720620876999, 0.9892280131939752,
        ],
        7 => &[
            0.5, 0.5027149505564014, 0.5054446430258502, 0.5081891574554764,
            0.5109485743270583, 0.5137229745593818, 0.5165124395106142, 0.5193170509806894,
            0.5221368912137069, 0.5249720429003435, 0.5278225891802786, 0.5306886136446309,
            0.5335702003384117, 0.5364674337629877, 0.5393803988785598, 0.5423091811066545,
            0.5452538663326288, 0.5482145409081883, 0.5511912916539204, 0.5541842058618393,
            0.5571933712979462, 0.5602188762048033, 0.5632608093041209, 0.566319269779814,
            0.5693943173783458, 0.5724860325887231, 0.5755946149764913, 0.5787201645382346,
            0.5818624293887887, 0.5850219584656471, 0.5881984958251406, 0.591392346683461,
            0.5946035575013605, 0.5978322844901228, 0.6010783657263515, 0.6043419431043025,
            0.6076236799902344, 0.6109235410905521, 0.6142402680534349, 0.6175753064736748,
            0.620928906036742, 0.624301417003498, 0.6276903785123455, 0.6310963416056474,
            0.6345254785958666, 0.6379774041621684, 0.6414350080393891, 0.6449016673462853,
            0.6484197773255048, 0.6519494235440618, 0.6554806057623822, 0.6590309541316167,
            0.6626183215798706, 0.6662208119068117, 0.6698337620266515, 0.6734648018119159,
            0.6771277734684463, 0.6808088519243828, 0.6845012114872953, 0.6882113490127399,
            0.6919549409819159, 0.6957170339299273, 0.6994898362691555, 0.7032881930540149,
            0.7071067811865475, 0.7109409262522719, 0.7148066691959849, 0.7186915456176411,
            0.7225904034885232, 0.7265162230820167, 0.7304588970903234, 0.7344113941525887,
            0.7384130729697496, 0.742435111820364, 0.7464538641456323, 0.7504914436779415,
            0.7545822137967112, 0.7586769382450135, 0.762799075372269, 0.7669463163824454,
            0.7711054127039704, 0.7752907089463246, 0.7795022001189185, 0.7837307324572845,
            0.7879904225539431, 0.7922673449229513, 0.7965710756711334, 0.8008920687786658,
            0.805245165974627, 0.8096156320566672, 0.8140137109286738, 0.8184291774671318,
            0.8228777390769823, 0.8273439797661134, 0.8318382901633681, 0.8363503284360101,
            0.8408964152537144, 0.8454604809881165, 0.8500531768592616, 0.8546638540388732,
            0.8593096490612387, 0.8639741282030884, 0.8686669176368529, 0.8733778099375125,
            0.8781260801866495, 0.8828928894405816, 0.8876882462632604, 0.8925022166714681,
            0.8973545375015533, 0.9022254252868866, 0.9071260877501991, 0.9120457633038208,
            0.9170040432046711, 0.9219817002865515, 0.9269895625416926, 0.9320170793416338,
            0.9370838170551498, 0.9421706649331766, 0.9472879907934827, 0.9524254621154538,
            0.9576032806985735, 0.9628019218241095, 0.9680308967461471, 0.9732799347859315,
            0.9785720620876999, 0.9838849683520958, 0.9892280131939752, 0.9946017579039926,
        ],
        8 => &[
            0.5, 0.5013556375251013, 0.5027149505564014, 0.5040779490592088,
            0.5054446430258502, 0.5068150424526111, 0.5081891574554764, 0.5095669981542462,
            0.5109485743270583, 0.5123338958360024, 0.5137229745593818, 0.5151158223600481,
            0.5165124395106142, 0.5179128375883652, 0.5193170509806894, 0.5207250893415385,
            0.5221368912137069, 0.5235524971778347, 0.5249720429003435, 0.5263954739517456,
            0.5278225891802786, 0.5292535014670196, 0.5306886136446309, 0.5321277398058019,
            0.5335702003384117, 0.5350171286513727, 0.5364674337629877, 0.537922125823734,
            0.5393803988785598, 0.5408421682002851, 0.5423091811066545, 0.5437796506835415,
            0.5452538663326288, 0.5467318378262693, 0.5482145409081883, 0.5497009458109818,
            0.5511912916539204, 0.5526855176997109, 0.5541842058618393, 0.5556866191918477,
            0.5571933712979462, 0.5587038904653383, 0.5602188762048033, 0.5617376520201697,
            0.5632608093041209, 0.5647876783009495, 0.566319269779814, 0.5678545956746303,
            0.5693943173783458, 0.5709378631639395, 0.5724860325887231, 0.5740381674803067,
            0.5755946149764913, 0.5771547714683196, 0.5787201645382346, 0.5802891762210371,
            0.5818624293887887, 0.5834398144619738, 0.5850219584656471, 0.5866082687369809,
            0.5881984958251406, 0.5897929085171479, 0.591392346683461, 0.5929955849138954,
            0.5946035575013605, 0.596215561320201, 0.5978322844901228, 0.5994532112130223,
            0.6010783657263515, 0.6027080773469174, 0.6043419431043025, 0.6059803659840126,
            0.6076236799902344, 0.6092711719908991, 0.6109235410905521, 0.6125801290199849,
            0.6142402680534349, 0.6159053385503669, 0.6175753064736748, 0.6192494620655603,
            0.620928906036742, 0.6226117744975897, 0.624301417003498, 0.6259951116248707,
            0.6276903785123455, 0.6293906639609544, 0.6310963416056474, 0.6328064213135438,
            0.6345254785958666, 0.6362488629565562, 0.6379774041621684, 0.6397103404529919,
            0.6414350080393891, 0.6431665387082528, 0.6449016673462853, 0.6466415441252097,
            0.6484197773255048, 0.6501985538012456, 0.6519494235440618, 0.6537360371403159,
            0.6554806057623822, 0.6572586707156278, 0.6590309541316167, 0.6608081065330741,
            0.6626183215798706, 0.6644328500016159, 0.6662208119068117, 0.6680449381329166,
            0.6698337620266515, 0.6716570874434275, 0.6734648018119159, 0.6753079703677805,
            0.6771277734684463, 0.6789821581462642, 0.6808088519243828, 0.6826704503934629,
            0.6845012114872953, 0.6863669687078089, 0.6882113490127399, 0.6900908119780444,
            0.6919549409819159, 0.6938542142546347, 0.6957170339299273, 0.6976147467716078,
            0.6994898362691555, 0.7014003137222518, 0.7032881930540149, 0.7052113766507296,
            0.7071067811865475, 0.7090383173622186, 0.7109409262522719, 0.7128799158420173,
            0.7148066691959849, 0.7167700863610147, 0.7186915456176411, 0.7206497431334046,
            0.7225904034885232, 0.7245678642157386, 0.7265162230820167, 0.7285025230906318,
            0.7304588970903234, 0.7324543491726859, 0.7344113941525887, 0.7364160135698662,
            0.7384130729697496, 0.7404270449755916, 0.742435111820364, 0.7444583578782328,
            0.7464538641456323, 0.7484866373411019, 0.7504914436779415, 0.7525336672795035,
            0.7545822137967112, 0.7566482971895177, 0.7586769382450135, 0.7607534074116829,
            0.762799075372269, 0.7648861650428935, 0.7669463163824454, 0.7690481473266491,
            0.7711054127039704, 0.7732177558458796, 0.7752907089463246, 0.7774135626504046,
            0.7795022001189185, 0.7816355897052811, 0.7837307324572845, 0.7858747273050105,
            0.7879904225539431, 0.7901451966737015, 0.7922673449229513, 0.7944328270321274,
            0.7965710756711334, 0.7987473199638114, 0.8008920687786658, 0.8030791498153661,
            0.805245165974627, 0.8074431808442795, 0.8096156320566672, 0.8118215703128187,
            0.8140137109286738, 0.8162405078155322, 0.8184291774671318, 0.8206569548124791,
            0.8228777390769823, 0.8251365703551548, 0.8273439797661134, 0.8296138250008398,
            0.8318382901633681, 0.8341192447517919, 0.8363503284360101, 0.8386358637108114,
            0.8408964152537144, 0.8432125368454744, 0.8454604809881165, 0.8477654063019961,
            0.8500531768592616, 0.8523989257485829, 0.8546638540388732, 0.8569868531611278,
            0.8593096490612387, 0.8616908227923805, 0.8639741282030884, 0.8663659929895022,
            0.8686669176368529, 0.8710774938265014, 0.8733778099375125, 0.8757978168676295,
            0.8781260801866495, 0.8805648279150395, 0.8828928894405816, 0.8853412535231275,
            0.8876882462632604, 0.890154411668869, 0.8925022166714681, 0.8949698851988716,
            0.8973545375015533, 0.8998402830094798, 0.9022254252868866, 0.9047307305654877,
            0.9071260877501991, 0.9096418465426253, 0.9120457633038208, 0.9145711052591621,
            0.9170040432046711, 0.9195389429857694, 0.9219817002865515, 0.9245362454059508,
            0.9269895625416926, 0.9295548748845608, 0.9320170793416338, 0.9345933017663328,
            0.9370838170551498, 0.9396861181028645, 0.9421706649331766, 0.9447881323804446,
            0.9472879907934827, 0.9499208200395267, 0.9524254621154538, 0.9550737350318831,
            0.9576032806985735, 0.9602673432572963, 0.9628019218241095, 0.9654819189004704,
            0.9680308967461471, 0.9707270696865725, 0.9732799347859315, 0.9759925266079602,
            0.9785720620876999, 0.9813013309986821, 0.9838849683520958, 0.9866310309750134,
            0.9892280131939752, 0.9920013318369795, 0.9946017579039926, 0.9974029367376888,
        ],
        _ => &[0.5], // Fallback for invalid schemas
    }
}

#[cfg(test)]
mod native_histogram_tests {
    use super::*;

    #[test]
    fn test_pick_schema() {
        // Test common bucket factors
        assert_eq!(pick_schema(1.1), 3);
        assert_eq!(pick_schema(1.05), 4);
        assert_eq!(pick_schema(2.0), 0);
        
        // Test edge cases
        assert_eq!(pick_schema(1.001), 8);  // Very small -> max schema
        assert_eq!(pick_schema(100.0), -2);  // Large -> negative schema
    }

    #[test]
    #[should_panic(expected = "bucket_factor must be greater than 1.0")]
    fn test_pick_schema_invalid() {
        pick_schema(1.0);
    }

    #[test]
    fn test_frexp() {
        let (frac, exp) = frexp(1.0);
        assert_eq!(frac, 0.5);
        assert_eq!(exp, 1);

        let (frac, exp) = frexp(2.0);
        assert_eq!(frac, 0.5);
        assert_eq!(exp, 2);

        let (frac, exp) = frexp(0.5);
        assert_eq!(frac, 0.5);
        assert_eq!(exp, 0);

        let (frac, exp) = frexp(0.0);
        assert_eq!(frac, 0.0);
        assert_eq!(exp, 0);
    }

    #[test]
    fn test_calculate_bucket_index() {
        // Test schema 0 (base 2 buckets)
        assert_eq!(calculate_bucket_index(1.0, 0), 0);
        assert_eq!(calculate_bucket_index(2.0, 0), 1);
        assert_eq!(calculate_bucket_index(4.0, 0), 2);

        // Test positive schema
        let idx1 = calculate_bucket_index(1.5, 3);
        let idx2 = calculate_bucket_index(1.6, 3);
        // Both should be in reasonable range
        assert!(idx1 >= 0);
        assert!(idx2 >= 0);
    }

    #[test]
    fn test_native_histogram_basic() {
        let histogram = NativeHistogram::new();
        histogram.observe(1.0);
        histogram.observe(2.0);
        histogram.observe(0.5);

        let (sum, count, zero_count, _, _, positive_buckets, _) = histogram.get();
        assert_eq!(count, 3);
        assert_eq!(sum, 3.5);
        assert_eq!(zero_count, 0);
        assert!(!positive_buckets.is_empty());
    }

    #[test]
    fn test_native_histogram_zero_bucket() {
        let histogram = NativeHistogram::with_zero_threshold(1.1, 0.1);
        histogram.observe(0.05);
        histogram.observe(0.001);
        histogram.observe(1.0);

        let (_sum, count, zero_count, _, _, positive_buckets, _) = histogram.get();
        assert_eq!(count, 3);
        assert_eq!(zero_count, 2); // Two values under threshold
        assert!(!positive_buckets.is_empty()); // One value above threshold
    }

    #[test]
    fn test_native_histogram_negative() {
        let histogram = NativeHistogram::new();
        histogram.observe(-1.0);
        histogram.observe(-2.0);
        histogram.observe(1.0);

        let (sum, count, zero_count, _, _, positive_buckets, negative_buckets) = histogram.get();
        assert_eq!(count, 3);
        assert_eq!(sum, -2.0);
        assert_eq!(zero_count, 0);
        assert!(!positive_buckets.is_empty());
        assert!(!negative_buckets.is_empty());
    }

    #[test]
    fn test_native_histogram_nan() {
        let histogram = NativeHistogram::new();
        histogram.observe(f64::NAN);
        histogram.observe(1.0);

        let (sum, count, _, _, _, _, _) = histogram.get();
        assert_eq!(count, 2);
        assert!(sum.is_nan());
    }

    #[test]
    fn test_native_histogram_clone() {
        let histogram = NativeHistogram::new();
        histogram.observe(1.0);
        
        let cloned = histogram.clone();
        cloned.observe(2.0);

        let (_, count, _, _, _, _, _) = histogram.get();
        assert_eq!(count, 2); // Both observations visible through shared state
    }
}
