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

#[cfg(test)]
mod slice_test;
