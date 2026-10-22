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
