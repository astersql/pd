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

#[cfg(test)]
mod slice_test;
