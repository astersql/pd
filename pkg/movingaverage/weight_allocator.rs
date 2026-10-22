// Copyright 2026 AsterSQL.

/// Allocates normalized weights across descending-weight segments.
#[derive(Debug, Default)]
pub struct WeightAllocator {
    weights: Vec<f64>,
}

impl WeightAllocator {
    /// Creates an allocator for `length` items split across `segment_count` segments.
    ///
    /// Any remainder is assigned to the leading segments. Invalid lengths or
    /// segment counts create an empty allocator whose [`Self::get`] returns zero.
    pub fn new(length: isize, segment_count: isize) -> Self {
        if length < 1 || segment_count < 1 {
            return Self::default();
        }

        let length = length as usize;
        let segment_count = segment_count as usize;
        let segment_length = length / segment_count;
        let segment_remainder = length % segment_count;
        let mut segment_lengths = Vec::with_capacity(segment_count);
        let mut unit_count = 0;

        for segment in 0..segment_count {
            let next_length = segment_length + usize::from(segment < segment_remainder);
            unit_count += (segment_count - segment) * next_length;
            segment_lengths.push(next_length);
        }

        let unit_weight = 1.0 / unit_count as f64;
        let mut weights = Vec::with_capacity(length);
        for (segment, segment_length) in segment_lengths.into_iter().enumerate() {
            weights.extend(std::iter::repeat_n(
                unit_weight * (segment_count - segment) as f64,
                segment_length,
            ));
        }

        Self { weights }
    }

    /// Returns the weight at `index`, or zero when `index` is out of bounds.
    pub fn get(&self, index: isize) -> f64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.weights.get(index))
            .copied()
            .unwrap_or(0.0)
    }
}
