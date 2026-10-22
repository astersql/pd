// Copyright 2026 AsterSQL.

use super::{Bucket, Window};

fn assert_bucket(bucket: &Bucket, points: &[f64], count: i64) {
    assert_eq!(bucket.points, points);
    assert_eq!(bucket.count, count);
}

#[test]
fn window_bucket_mutations() {
    let mut bucket = Bucket::default();
    bucket.append(2.0);
    bucket.append(9.0);
    bucket.add(1, -3.0);
    assert_bucket(&bucket, &[2.0, 6.0], 3);
    let capacity = bucket.points.capacity();
    bucket.reset();
    assert_bucket(&bucket, &[], 0);
    assert_eq!(bucket.points.capacity(), capacity);
    bucket.append(-1.0);
    assert_bucket(&bucket, &[-1.0], 1);

    let mut window = Window::new(3);
    assert_eq!(window.size(), 3);
    for offset in 0..3 {
        assert_bucket(window.bucket(offset), &[], 0);
        window.append(offset, 1.0);
    }
    window.append(4, 9.0);
    window.add(7, 2.0);
    // Go adds to point zero even when a later point exists.
    assert_bucket(window.bucket(1), &[3.0, 9.0], 3);
    assert_bucket(window.bucket(10), &[3.0, 9.0], 3);
    window.reset_bucket(7);
    assert_bucket(window.bucket(1), &[], 0);
    assert_bucket(window.bucket(0), &[1.0], 1);
    assert_bucket(window.bucket(2), &[1.0], 1);
    window.add(4, -2.0);
    window.add(1, 3.0);
    assert_bucket(window.bucket(1), &[1.0], 2);
    window.reset_buckets(5, 2);
    assert_bucket(window.bucket(2), &[], 0);
    assert_bucket(window.bucket(0), &[], 0);
    assert_bucket(window.bucket(1), &[1.0], 2);
    window.reset_buckets(1, 0);
    assert_bucket(window.bucket(1), &[1.0], 2);
    window.reset_buckets(2, 7);
    for offset in 0..3 {
        assert_bucket(window.bucket(offset), &[], 0);
        window.append(offset, offset as f64);
    }
    window.reset_window();
    for offset in 0..3 {
        assert_bucket(window.bucket(offset), &[], 0);
    }
    let mut single = Window::new(1);
    single.add(8, 4.0);
    single.append(9, 2.0);
    single.add(10, 3.0);
    assert_bucket(single.bucket(0), &[7.0, 2.0], 3);
    single.reset_buckets(4, 2);
    assert_bucket(single.bucket(0), &[], 0);
    let mut empty = Window::new(0);
    assert_eq!(empty.size(), 0);
    empty.reset_window();
    empty.reset_buckets(0, 0);
}

#[test]
#[should_panic]
fn bucket_add_requires_existing_point() {
    Bucket::default().add(0, 1.0);
}

#[test]
#[should_panic]
fn empty_window_bucket_panics_like_go() {
    Window::new(0).bucket(0);
}
