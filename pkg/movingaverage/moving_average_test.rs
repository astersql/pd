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
