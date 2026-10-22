// Copyright 2026 AsterSQL.

use super::{Window, avg, count, max, min, sum};

#[test]
fn iterator_and_reducers() {
    let mut w = Window::new(3);
    w.append(0, 2.0);
    w.append(0, 4.0);
    w.add(0, 2.0);
    w.append(2, -3.0);
    let mut i = w.iterator(5, 5);
    for (points, count) in [
        (&[-3.0][..], 1),
        (&[4.0, 4.0], 3),
        (&[], 0),
        (&[-3.0], 1),
        (&[4.0, 4.0], 3),
    ] {
        assert!(i.has_next());
        assert!(i.has_next()); // Checking availability does not consume a bucket.
        let b = i.bucket();
        assert_eq!(b.points, points);
        assert_eq!(b.count, count);
    }
    assert!(!i.has_next());
    assert!(std::panic::catch_unwind(move || i.bucket()).is_err());
    assert_eq!(sum(w.iterator(2, 3)), 5.0);
    assert_eq!(avg(w.iterator(2, 3)), 5.0 / 3.0);
    assert_eq!(min(w.iterator(2, 3)), -3.0);
    assert_eq!(max(w.iterator(2, 3)), 4.0);
    assert_eq!(count(w.iterator(2, 3)), 4.0);
    assert_eq!(sum(w.iterator(2, 5)), 10.0);
    assert_eq!(count(w.iterator(2, 5)), 8.0);
    for (offset, length) in [(1, 1), (0, 0)] {
        assert_eq!(sum(w.iterator(offset, length)), 0.0);
        assert!(avg(w.iterator(offset, length)).is_nan());
        assert_eq!(min(w.iterator(offset, length)), 0.0);
        assert_eq!(max(w.iterator(offset, length)), 0.0);
        assert_eq!(count(w.iterator(offset, length)), 0.0);
    }
    let mut single = Window::new(1);
    single.append(0, -5.0);
    single.append(0, -2.0);
    assert_eq!(max(single.iterator(9, 1)), -2.0);
    single.reset_window();
    single.append(0, 5.0);
    single.append(0, 2.0);
    assert_eq!(min(single.iterator(0, 1)), 2.0);
    for points in [[f64::NAN, 1.0], [1.0, f64::NAN]] {
        single.reset_window();
        for p in points {
            single.append(0, p);
        }
        assert!(sum(single.iterator(0, 1)).is_nan());
        assert!(avg(single.iterator(0, 1)).is_nan());
        if points[0].is_nan() {
            assert!(min(single.iterator(0, 1)).is_nan());
            assert!(max(single.iterator(0, 1)).is_nan());
        } else {
            assert_eq!(min(single.iterator(0, 1)), 1.0);
            assert_eq!(max(single.iterator(0, 1)), 1.0);
        }
    }
    assert!(
        std::panic::catch_unwind(|| {
            let w = Window::new(0);
            w.iterator(0, 0);
        })
        .is_err()
    );
}
