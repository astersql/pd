// Copyright 2026 AsterSQL.
// Copyright 2022 The Go Authors. All rights reserved.
// See GO-SORT-LICENSE for the upstream BSD license.
// Adapted from Go 1.26 sort/zsortfunc.go (Go Authors, BSD-3-Clause).
// Keep Go's unstable equal-key permutation: it affects unsorted item order.
pub(crate) fn sort<T>(data: &mut [(Vec<u8>, bool, T)]) {
    let n = data.len();
    pdq(data, 0, n, usize::BITS - n.leading_zeros());
}
fn less<T>(d: &[(Vec<u8>, bool, T)], a: usize, b: usize) -> bool {
    d[a].0 < d[b].0
}
fn insertion<T>(d: &mut [(Vec<u8>, bool, T)], a: usize, b: usize) {
    for i in a + 1..b {
        let mut j = i;
        while j > a && less(d, j, j - 1) {
            d.swap(j, j - 1);
            j -= 1;
        }
    }
}
fn sift<T>(d: &mut [(Vec<u8>, bool, T)], mut root: usize, hi: usize, first: usize) {
    loop {
        let mut child = 2 * root + 1;
        if child >= hi {
            return;
        }
        if child + 1 < hi && less(d, first + child, first + child + 1) {
            child += 1;
        }
        if !less(d, first + root, first + child) {
            return;
        }
        d.swap(first + root, first + child);
        root = child;
    }
}
fn heap<T>(d: &mut [(Vec<u8>, bool, T)], a: usize, b: usize) {
    let n = b - a;
    for i in (0..=(n - 1) / 2).rev() {
        sift(d, i, n, a);
    }
    for i in (0..n).rev() {
        d.swap(a, a + i);
        sift(d, 0, i, a);
    }
}
fn order<T>(d: &[(Vec<u8>, bool, T)], a: usize, b: usize, swaps: &mut usize) -> (usize, usize) {
    if less(d, b, a) {
        *swaps += 1;
        (b, a)
    } else {
        (a, b)
    }
}
fn median<T>(d: &[(Vec<u8>, bool, T)], a: usize, b: usize, c: usize, s: &mut usize) -> usize {
    let (a, b) = order(d, a, b, s);
    let (b, _) = order(d, b, c, s);
    let (_, b) = order(d, a, b, s);
    b
}
fn pivot<T>(d: &[(Vec<u8>, bool, T)], a: usize, b: usize) -> (usize, usize) {
    let n = b - a;
    let (mut i, mut j, mut k) = (a + n / 4, a + n / 4 * 2, a + n / 4 * 3);
    let mut s = 0;
    if n >= 8 {
        if n >= 50 {
            i = median(d, i - 1, i, i + 1, &mut s);
            j = median(d, j - 1, j, j + 1, &mut s);
            k = median(d, k - 1, k, k + 1, &mut s);
        }
        j = median(d, i, j, k, &mut s);
    }
    (
        j,
        if s == 0 {
            1
        } else if s == 12 {
            2
        } else {
            0
        },
    )
}
fn partial<T>(d: &mut [(Vec<u8>, bool, T)], a: usize, b: usize) -> bool {
    let mut i = a + 1;
    for _ in 0..5 {
        while i < b && !less(d, i, i - 1) {
            i += 1;
        }
        if i == b {
            return true;
        }
        if b - a < 50 {
            return false;
        }
        d.swap(i, i - 1);
        if i - a >= 2 {
            let mut j = i - 1;
            while j >= 1 && less(d, j, j - 1) {
                d.swap(j, j - 1);
                j -= 1;
            }
        }
        if b - i >= 2 {
            let mut j = i + 1;
            while j < b && less(d, j, j - 1) {
                d.swap(j, j - 1);
                j += 1;
            }
        }
    }
    false
}
fn patterns<T>(d: &mut [(Vec<u8>, bool, T)], a: usize, b: usize) {
    let n = b - a;
    if n >= 8 {
        let mut r = n as u64;
        let modulus = 1usize << (usize::BITS - n.leading_zeros());
        for i in a + n / 4 * 2 - 1..=a + n / 4 * 2 + 1 {
            r ^= r << 13;
            r ^= r >> 7;
            r ^= r << 17;
            let mut other = r as usize & (modulus - 1);
            if other >= n {
                other -= n;
            }
            d.swap(i, a + other);
        }
    }
}
fn partition<T>(
    d: &mut [(Vec<u8>, bool, T)],
    a: usize,
    b: usize,
    p: usize,
    equal: bool,
) -> (usize, bool) {
    d.swap(a, p);
    let (mut i, mut j) = (a + 1, b - 1);
    let mut untouched = true;
    loop {
        while i <= j && if equal { !less(d, a, i) } else { less(d, i, a) } {
            i += 1;
        }
        while i <= j && if equal { less(d, a, j) } else { !less(d, j, a) } {
            j -= 1;
        }
        if i > j {
            break;
        }
        d.swap(i, j);
        i += 1;
        j -= 1;
        untouched = false;
    }
    if equal {
        (i, untouched)
    } else {
        d.swap(j, a);
        (j, untouched)
    }
}
fn pdq<T>(d: &mut [(Vec<u8>, bool, T)], mut a: usize, mut b: usize, mut limit: u32) {
    let (mut balanced, mut partitioned) = (true, true);
    loop {
        let n = b - a;
        if n <= 12 {
            insertion(d, a, b);
            return;
        }
        if limit == 0 {
            heap(d, a, b);
            return;
        }
        if !balanced {
            patterns(d, a, b);
            limit -= 1;
        }
        let (mut p, mut hint) = pivot(d, a, b);
        if hint == 2 {
            d[a..b].reverse();
            p = b - 1 - (p - a);
            hint = 1;
        }
        if balanced && partitioned && hint == 1 && partial(d, a, b) {
            return;
        }
        if a > 0 && !less(d, a - 1, p) {
            a = partition(d, a, b, p, true).0;
            continue;
        }
        let (mid, already) = partition(d, a, b, p, false);
        partitioned = already;
        let (left, right) = (mid - a, b - mid);
        if left < right {
            balanced = left >= n / 8;
            pdq(d, a, mid, limit);
            a = mid + 1;
        } else {
            balanced = right >= n / 8;
            pdq(d, mid + 1, b, limit);
            b = mid;
        }
    }
}
