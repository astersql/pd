// Copyright 2026 AsterSQL.

use crate::Ema;

const TOLERANCE: f64 = 1e-7;

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= TOLERANCE,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn ema_behavior() {
    let data = [1.0, 1.0, 1.0, 1.0, 5.0, 1.0, 1.0, 1.0];
    let expected = [1.0, 1.0, 1.0, 1.0, 4.6, 1.36, 1.036, 1.0036];
    let mut ema = Ema::with_decay(0.9);

    assert_eq!(ema.get(), 0.0);
    for (value, expected) in data.into_iter().zip(expected) {
        ema.add(value);
        assert_eq!(ema.get_instantaneous(), value);
        assert_close(ema.get(), expected);
    }

    ema.reset();
    assert_eq!(ema.get(), 0.0);
    assert_eq!(ema.get_instantaneous(), 1.0);

    ema.set(2.0);
    assert_eq!(ema.get(), 2.0);
    assert_eq!(ema.get_instantaneous(), 1.0);
    ema.add(5.0);
    assert_close(ema.get(), 4.7);
    assert_eq!(ema.get_instantaneous(), 5.0);

    let mut default_ema = Ema::new();
    for _ in 0..25 {
        default_ema.add(1.0);
    }
    default_ema.add(5.0);
    assert_close(default_ema.get(), 1.3);

    for invalid_decay in [0.0, 1.0, -0.1, 1.1, f64::NAN] {
        let mut fallback = Ema::with_decay(invalid_decay);
        for _ in 0..25 {
            fallback.add(1.0);
        }
        fallback.add(5.0);
        assert_close(fallback.get(), 1.3);
    }
}

#[test]
fn wma_behavior() {
    use crate::Wma;
    let data = [1.0, 1.0, 1.0, 1.0, 5.0, 1.0, 1.0, 1.0];
    let expected = [1.0, 1.0, 1.0, 1.0, 2.33333333, 2.06666667, 1.8, 1.533333333];
    let mut wma = Wma::with_size(5);
    assert_eq!(wma.get(), 0.0);
    assert_eq!(wma.get_instantaneous(), 0.0);
    for (value, expected) in data.into_iter().zip(expected) {
        wma.add(value);
        assert_eq!(wma.get_instantaneous(), value);
        assert_close(wma.get(), expected);
    }
    wma.reset();
    assert_eq!(wma.get(), 0.0);
    assert_eq!(wma.get_instantaneous(), 0.0);
    wma.add(7.0);
    assert_eq!(wma.get(), 7.0);
    wma.set(-3.0);
    assert_eq!(wma.get(), -3.0);
    assert_eq!(wma.get_instantaneous(), -3.0);
    wma.add(6.0);
    assert_close(wma.get(), 2.0);

    for (mut average, size) in [
        (Wma::new(), 10),
        (Wma::default(), 10),
        (Wma::with_size(0), 10),
        (Wma::with_size(1), 10),
        (Wma::with_size(-5), 10),
        (Wma::with_size(2), 2),
        (Wma::with_size(7), 7),
    ] {
        average.reset();
        let mut history = Vec::new();
        for i in 0..100 {
            let value = ((i * 17) % 31) as f64 - 15.0;
            average.add(value);
            history.push(value);
            let window = &history[history.len().saturating_sub(size)..];
            let first_weight = size - window.len() + 1;
            let numerator: f64 = window
                .iter()
                .enumerate()
                .map(|(i, v)| *v * (first_weight + i) as f64)
                .sum();
            let denominator: usize = (first_weight..=size).sum();
            assert_close(average.get(), numerator / denominator as f64);
            assert_eq!(average.get_instantaneous(), value);
        }
        average.reset();
        assert_eq!(average.get(), 0.0);
        assert_eq!(average.get_instantaneous(), 0.0);
    }
}

#[test]
fn hma_behavior() {
    use crate::Hma;
    let data = [1.0, 1.0, 1.0, 1.0, 5.0, 1.0, 1.0, 1.0];
    let expected = [1.0, 1.0, 1.0, 1.0, 3.6666667, 3.4, 1.0, 0.3777778];
    let mut hma = Hma::with_size(5.0);
    assert_eq!(hma.get(), 0.0);
    assert_eq!(hma.get_instantaneous(), 0.0);
    for _ in 0..3 {
        for (value, expected) in data.into_iter().zip(expected) {
            hma.add(value);
            assert_close(hma.get(), expected);
            assert_eq!(hma.get_instantaneous(), value);
        }
        hma.reset();
        assert_eq!(hma.get(), 0.0);
        assert_eq!(hma.get_instantaneous(), 0.0);
    }
    hma.add(100.0);
    hma.set(data[0]);
    assert_close(hma.get(), expected[0]);
    assert_eq!(hma.get_instantaneous(), data[0]);
    for (value, expected) in data[1..].iter().zip(&expected[1..]) {
        hma.add(*value);
        assert_close(hma.get(), *expected);
    }

    // Direct weighted sums independently check Wma's running recurrence.
    fn weighted(history: &[f64], size: usize) -> f64 {
        let size = if size <= 1 { 10 } else { size };
        let window = &history[history.len().saturating_sub(size)..];
        let first = size - window.len() + 1;
        let sum: f64 = window
            .iter()
            .enumerate()
            .map(|(i, value)| value * (first + i) as f64)
            .sum();
        sum / (first..=size).sum::<usize>() as f64
    }
    for (mut hma, size) in [
        (Hma::new(), 10),
        (Hma::default(), 10),
        (Hma::with_size(-3.0), 10),
        (Hma::with_size(0.0), 10),
        (Hma::with_size(1.0), 10),
        (Hma::with_size(f64::NAN), 10),
        (Hma::with_size(1.9), 1),
        (Hma::with_size(2.0), 2),
        (Hma::with_size(3.0), 3),
        (Hma::with_size(5.9), 5),
        (Hma::with_size(16.0), 16),
    ] {
        let mut history = Vec::new();
        let mut combined = Vec::new();
        for i in 0..100 {
            let value = ((i * 17) % 31) as f64 - 15.0;
            history.push(value);
            combined.push(2.0 * weighted(&history, size / 2) - weighted(&history, size));
            hma.add(value);
            assert_close(
                hma.get(),
                weighted(&combined, (size as f64).sqrt() as usize),
            );
            assert_eq!(hma.get_instantaneous(), value);
        }
        hma.set(-7.0);
        assert_close(hma.get(), -7.0);
        assert_eq!(hma.get_instantaneous(), -7.0);
        hma.reset();
        assert_eq!(hma.get(), 0.0);
        assert_eq!(hma.get_instantaneous(), 0.0);
    }
}

#[test]
fn median_filter_go_sequence() {
    use crate::MedianFilter;
    let mut filter = MedianFilter::new(5);
    assert_eq!(filter.get(), 0.0);
    let data = [2.0, 4.0, 2.0, 800.0, 600.0, 6.0, 3.0];
    let expected = [2.0, 3.0, 2.0, 3.0, 4.0, 6.0, 6.0];
    for _ in 0..3 {
        filter.reset();
        assert_eq!(filter.get(), 0.0);
        for (value, expected) in data.into_iter().zip(expected) {
            filter.add(value);
            assert_close(filter.get(), expected);
            assert_eq!(filter.get_instantaneous(), value);
        }
        filter.set(data[0]);
        assert_eq!(filter.get(), data[0]);
        for (value, expected) in data.into_iter().zip(expected).skip(1) {
            filter.add(value);
            assert_close(filter.get(), expected);
        }
    }
}

#[test]
fn median_filter_windows_and_clone() {
    use crate::MedianFilter;
    for size in [1, 2, 4, 5, 10] {
        let mut filter = MedianFilter::new(size);
        let mut history = Vec::new();
        for i in 0..100 {
            let value = ((i * 17) % 31) as f64 - 15.0;
            filter.add(value);
            history.push(value);
            let mut window = history[history.len().saturating_sub(size as usize)..].to_vec();
            window.sort_by(f64::total_cmp);
            let middle = window.len() / 2;
            let expected = if window.len() % 2 == 1 {
                window[middle]
            } else {
                (window[middle - 1] + window[middle]) / 2.0
            };
            assert_close(filter.get(), expected);
            assert_eq!(filter.get_instantaneous(), value);
        }
        let mut cloned = filter.clone();
        assert_eq!(cloned.get(), filter.get());
        assert_eq!(cloned.get_instantaneous(), filter.get_instantaneous());
        cloned.set(100.0);
        assert_eq!(cloned.get(), 100.0);
        assert_ne!(filter.get(), 100.0);
        let original = filter.get();
        cloned.add(200.0);
        assert_eq!(filter.get(), original);
        filter.reset();
        assert_eq!(filter.get(), 0.0);
        filter.add(-9.0);
        assert_eq!(filter.get(), -9.0);
        filter.set(8.0);
        assert_eq!(filter.get_instantaneous(), 8.0);
        filter.add(4.0);
        assert_eq!(filter.get(), if size == 1 { 4.0 } else { 6.0 });
    }
}

#[test]
fn median_filter_empty_and_invalid_windows() {
    use crate::MedianFilter;
    assert!(std::panic::catch_unwind(|| MedianFilter::new(-1)).is_err());
    let mut zero = MedianFilter::new(0);
    assert_eq!(zero.get(), 0.0);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| zero.add(1.0))).is_err());
    let mut filter = MedianFilter::new(3);
    assert_eq!(filter.get_instantaneous(), 0.0);
    for value in [7.0, 8.0, 9.0] {
        filter.add(value);
    }
    filter.reset();
    // Go retains records and uses wrapping uint64 subtraction on empty reads.
    assert_eq!(filter.get_instantaneous(), 7.0);
}
