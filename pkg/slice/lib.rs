// Copyright 2026 AsterSQL.

/// Returns whether any index in `slice` satisfies `predicate`.
pub fn any_of<T, F>(slice: &[T], mut predicate: F) -> bool
where
    F: FnMut(usize) -> bool,
{
    (0..slice.len()).any(&mut predicate)
}

/// Returns whether no index in `slice` satisfies `predicate`.
pub fn none_of<T, F>(slice: &[T], predicate: F) -> bool
where
    F: FnMut(usize) -> bool,
{
    !any_of(slice, predicate)
}

/// Returns whether every index in `slice` satisfies `predicate`.
pub fn all_of<T, F>(slice: &[T], mut predicate: F) -> bool
where
    F: FnMut(usize) -> bool,
{
    none_of(slice, |index| !predicate(index))
}

/// Returns whether `slice` contains `value`.
pub fn contains<T>(slice: &[T], value: &T) -> bool
where
    T: PartialEq,
{
    slice.iter().any(|item| item == value)
}

/// Removes every element equal to `value` while preserving the remaining order.
pub fn remove<T>(slice: &mut Vec<T>, value: &T)
where
    T: PartialEq,
{
    slice.retain(|item| item != value);
}

/// Returns whether two slices contain equal elements with equal multiplicities.
pub fn equal_without_order<T>(left: &[T], right: &[T]) -> bool
where
    T: PartialEq,
{
    if left.len() != right.len() {
        return false;
    }

    let mut matched = vec![false; right.len()];
    left.iter().all(|left_item| {
        right
            .iter()
            .enumerate()
            .find(|(index, right_item)| !matched[*index] && left_item == *right_item)
            .map(|(index, _)| matched[index] = true)
            .is_some()
    })
}

/// Returns whether a sorted slice contains adjacent duplicate elements.
pub fn has_dup_in_sorted<T>(slice: &[T]) -> bool
where
    T: PartialEq,
{
    slice.windows(2).any(|pair| pair[0] == pair[1])
}

/// The error returned when a slice cannot be split into batches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitIntoBatchesError {
    /// A batch must contain at least one item.
    ZeroBatchSize,
}

impl std::fmt::Display for SplitIntoBatchesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroBatchSize => formatter.write_str("batch size must be greater than zero"),
        }
    }
}

impl std::error::Error for SplitIntoBatchesError {}

/// Splits `slice` into ordered, borrowed batches of at most `batch_size` items.
pub fn split_into_batches<T>(
    slice: &[T],
    batch_size: usize,
) -> Result<Vec<&[T]>, SplitIntoBatchesError> {
    if batch_size == 0 {
        return Err(SplitIntoBatchesError::ZeroBatchSize);
    }

    Ok(slice.chunks(batch_size).collect())
}

#[cfg(test)]
mod slice_test;
