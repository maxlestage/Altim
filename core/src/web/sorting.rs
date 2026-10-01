//! The stable sorts of the front, through one instance of the standard sort: `sort_by` monomorphises the whole sort
//! (merge, small-sort and pivot routines) for every element type and comparison, about 4 % of each .wasm. Here the
//! standard stable sort orders the indices once (`order`, one instance), then the slice is put in that order. Same
//! result as `sort_by` (stable, same comparisons); for the front's short lists, never for the engines' long series.
use std::cmp::Ordering;

/// The indices `0..n` in the stable order of `cmp`.
fn order(n: usize, cmp: &mut dyn FnMut(usize, usize) -> Ordering) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| cmp(a, b));
    idx
}

/// `v[i]` becomes the former `v[idx[i]]` (following each cycle of the permutation).
fn permute<T>(v: &mut [T], mut idx: Vec<usize>) {
    for i in 0..v.len() {
        let mut j = i;
        while idx[j] != usize::MAX {
            let k = std::mem::replace(&mut idx[j], usize::MAX);
            if k == i {
                break;
            }
            v.swap(j, k);
            j = k;
        }
    }
}

pub trait Sorting<T> {
    /// `sort_by` (stable).
    fn sort_by_dyn(&mut self, cmp: impl FnMut(&T, &T) -> Ordering);
    /// `sort_by_key` (stable).
    fn sort_by_key_dyn<K: Ord>(&mut self, key: impl FnMut(&T) -> K);
    /// `sort` (stable).
    fn sort_dyn(&mut self)
    where
        T: Ord;
}

impl<T> Sorting<T> for [T] {
    fn sort_by_dyn(&mut self, mut cmp: impl FnMut(&T, &T) -> Ordering) {
        if self.len() < 2 {
            return;
        }
        let idx = {
            let s = &*self;
            order(s.len(), &mut |a, b| cmp(&s[a], &s[b]))
        };
        permute(self, idx);
    }

    fn sort_by_key_dyn<K: Ord>(&mut self, mut key: impl FnMut(&T) -> K) {
        self.sort_by_dyn(|a, b| key(a).cmp(&key(b)));
    }

    fn sort_dyn(&mut self)
    where
        T: Ord,
    {
        self.sort_by_dyn(T::cmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same order as the standard stable sort, ties included, on many lists.
    #[test]
    #[allow(clippy::unnecessary_sort_by)]
    fn as_the_standard_sort() {
        let mut seed = 7u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for n in 0..200 {
            let v: Vec<(u32, usize)> = (0..n).map(|i| (next() % 7, i)).collect();
            let (mut a, mut b) = (v.clone(), v.clone());
            a.sort_by(|x, y| x.0.cmp(&y.0));
            b.sort_by_dyn(|x, y| x.0.cmp(&y.0));
            assert_eq!(a, b, "{n}");
            let (mut a, mut b) = (v.clone(), v.clone());
            a.sort_by_key(|x| std::cmp::Reverse(x.0));
            b.sort_by_key_dyn(|x| std::cmp::Reverse(x.0));
            assert_eq!(a, b);
            let (mut a, mut b) = (v.clone(), v);
            a.sort();
            b.sort_dyn();
            assert_eq!(a, b);
        }
        let mut s = vec!["b".to_string(), "a".into(), "c".into()];
        s.sort_dyn();
        assert_eq!(s, ["a", "b", "c"]);
    }
}
