// Copyright 2026 AsterSQL.

use super::WeightAllocator;

const EPSILON: f64 = 1e-8;

fn assert_sum(allocator: &WeightAllocator, length: isize) {
    let sum = (0..length).map(|index| allocator.get(index)).sum::<f64>();
    assert!((1.0 - sum).abs() < EPSILON);
}

fn assert_ratio(left: f64, right: f64, ratio: f64) {
    assert!((left / right - ratio).abs() < EPSILON);
}

#[test]
fn weight_allocator() {
    let allocator = WeightAllocator::new(1, 1);
    assert_eq!(allocator.get(0), 1.0);
    assert_sum(&allocator, 1);

    let allocator = WeightAllocator::new(10, 10);
    assert_sum(&allocator, 10);
    for index in 0..10 {
        assert_eq!(allocator.get(index), 1.0 / 55.0 * (10 - index) as f64);
    }
    assert_ratio(allocator.get(0), allocator.get(9), 10.0);
    assert_ratio(allocator.get(0), allocator.get(5), 2.0);

    let allocator = WeightAllocator::new(10, 1);
    assert_sum(&allocator, 10);
    assert_eq!(allocator.get(0), 1.0 / 10.0);
    assert_eq!(allocator.get(7), 1.0 / 10.0);

    let allocator = WeightAllocator::new(10, 3);
    assert_sum(&allocator, 10);
    assert_ratio(allocator.get(0), allocator.get(3), 1.0);
    assert_ratio(allocator.get(3), allocator.get(9), 3.0);
    assert_ratio(allocator.get(6), allocator.get(9), 2.0);
    assert_eq!(allocator.get(10), 0.0);

    for (length, segments, index) in [(100, 0, 88), (0, 0, 0), (0, 100, 10), (0, 100, -2)] {
        assert_eq!(WeightAllocator::new(length, segments).get(index), 0.0);
    }

    let allocator = WeightAllocator::new(0, 2);
    assert_eq!(allocator.get(0), 0.0);
    assert_eq!(allocator.get(1), 0.0);
    assert_eq!(allocator.get(2), 0.0);
}
