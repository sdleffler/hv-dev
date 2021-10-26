//! `no_std` port and optimization of the oddly named `voluntary-servitude` crate's
//! inscrutably-named `VoluntaryServitude` type (renamed here as [`MonotonicList`].)

use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use core::{
    fmt::{self, Debug, Formatter},
    iter::FusedIterator,
};
use core::{iter::Extend, iter::FromIterator, ptr::NonNull};

use alloc::{boxed::Box, vec::Vec};

use crate::atom::AtomSetOnce;

pub struct Node<T> {
    /// Inner value
    value: T,
    /// Next node in chain
    next: AtomSetOnce<Box<Node<T>>>,
}

impl<T> Node<T> {
    /// Returns reference to inner value
    #[inline]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Creates new node with inner value
    #[inline]
    pub fn new(value: T) -> Self {
        let next = AtomSetOnce::empty();
        Self { value, next }
    }

    /// Gets next pointer
    #[inline]
    pub fn next(&self) -> Option<&Self> {
        self.next.get(Ordering::Relaxed)
    }

    /// Inserts next as if there was None
    #[inline]
    pub fn try_store_next(&self, node: Box<Self>) -> bool {
        self.next.set_if_none(node, Ordering::Relaxed).is_none()
    }
}

/// Default Drop is recursive and causes a stackoverflow easily
impl<T> Drop for Node<T> {
    #[inline]
    fn drop(&mut self) {
        let mut node = self.next.atom().take(Ordering::Relaxed);
        while let Some(mut n) = node {
            node = n.next.atom().take(Ordering::Relaxed);
        }
    }
}

impl<T: Debug> Debug for Node<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Node")
            .field("value", &self.value)
            .field("next", &self.next.get(Ordering::Relaxed))
            .finish()
    }
}

pub struct MonotonicList<T> {
    /// Number of elements inside `Inner`
    size: AtomicUsize,
    /// First node in `Inner`
    first_node: AtomSetOnce<Box<Node<T>>>,
    /// Last node in `Inner`
    last_node: AtomicPtr<Node<T>>,
}

impl<T> Default for MonotonicList<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T> MonotonicList<T> {
    /// Atomically extracts pointer to first node
    #[inline]
    fn first_node(&self) -> Option<NonNull<Node<T>>> {
        let nn = NonNull::new(self.first_node.get(Ordering::Relaxed)? as *const _ as *mut _);
        nn
    }

    /// Atomically extracts pointer to last node
    #[inline]
    fn last_node(&self) -> Option<NonNull<Node<T>>> {
        NonNull::new(self.last_node.load(Ordering::Relaxed))
    }

    /// Set first node in chain
    #[inline]
    fn set_first(&self, node: Box<Node<T>>) -> Option<Box<Node<T>>> {
        self.first_node.set_if_none(node, Ordering::Relaxed)
    }

    /// Swaps last node, returning old one
    #[inline]
    fn swap_last(&self, ptr: *mut Node<T>) -> Option<NonNull<Node<T>>> {
        NonNull::new(self.last_node.swap(ptr, Ordering::Relaxed))
    }

    /// Unsafelly push a `Node<T>` chain to `Inner<T>`
    ///
    /// # Safety
    ///
    /// It's unsafe because we can't be sure of the ownership of `first` or `last`.
    ///
    /// To call this you must ensure the objects pointed by `first` and `last` are owned by no-one, so `Inner` will take its ownership.
    ///
    /// Nobody can use these pointers (without using `Inner`'s API) or drop them after calling this function
    ///
    /// (The objects pointed must exist while `Inner` exists and they can't be accessed after)
    #[inline]
    pub unsafe fn push_chain(&self, first: *mut Node<T>, last: *mut Node<T>, length: usize) {
        if let Some(nn) = self.swap_last(last) {
            // To call `Box::from_raw` unsafe is needed
            // But since `Inner` owns what they point to, it can be sure they will exist while `Inner` does
            // (as long as `push_chain` was properly called)
            #[allow(unused)]
            let success = nn.as_ref().try_store_next(Box::from_raw(first));
            debug_assert!(success);
        } else {
            // To call `Box::from_raw` you must make sure `Inner` now owns the `Node<T>`
            let result = self.set_first(Box::from_raw(first));
            debug_assert!(result.is_none());
        }

        let _ = self.size.fetch_add(length, Ordering::Relaxed);
    }

    #[inline]
    /// Extracts chain and drops itself without dropping it
    pub fn into_inner(mut self) -> (usize, *mut Node<T>, *mut Node<T>) {
        let size = self.size.into_inner();
        let first = self
            .first_node
            .atom()
            .take(Ordering::Relaxed)
            .map_or(core::ptr::null_mut(), Box::into_raw);
        let last = self.last_node.into_inner();
        (size, first, last)
    }
}

impl<T> FromIterator<T> for MonotonicList<T> {
    #[inline]
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let inner = Self::default();
        for element in iter {
            inner.push(element);
        }
        inner
    }
}

impl<T> MonotonicList<T> {
    /// Creates a new empty `MonotonicList`.
    ///
    /// ```rust
    /// # use hv_sync::monotonic_list::MonotonicList;
    /// let list: MonotonicList<()> = MonotonicList::new();
    /// assert!(list.is_empty());
    /// ```
    #[inline]
    pub const fn new() -> Self {
        Self {
            size: AtomicUsize::new(0),
            first_node: AtomSetOnce::empty(),
            last_node: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// Inserts an element after the last node.
    ///
    /// ```rust
    /// # use hv_sync::monotonic_list::MonotonicList;
    /// let list = MonotonicList::new();
    /// let mut iter = list.iter();
    ///
    /// list.push(3);
    /// // Iter doesn't grow if it's empty (originally empty or was consumed)
    /// assert!(iter.is_empty());
    ///
    /// iter = list.iter();
    /// list.push(8);
    /// // Iter grows if it has not been consumed
    /// assert_eq!(iter.collect::<Vec<_>>(), vec![&3, &8]);
    /// ```
    #[inline]
    pub fn push(&self, value: T) -> &T {
        let ptr = Box::into_raw(Box::new(Node::new(value)));
        // We own `Node<T>` so we can pass its ownership to `push_chain`
        // And we don't drop it
        unsafe {
            self.push_chain(ptr, ptr, 1);
            &(*ptr).value
        }
    }

    /// Get a lock-free iterator over the `MonotonicList`, which iterates over the *current* state
    /// of the `MonotonicList` (not just the state at which it was created.)
    ///
    /// ```rust
    /// # use hv_sync::monotonic_list::MonotonicList;
    /// let list = MonotonicList::new();
    /// list.push(3);
    /// list.push(2);
    /// assert_eq!(list.iter().collect::<Vec<_>>(), vec![&3, &2]);
    ///
    /// for (element, expected) in list.iter().zip(&[3, 2][..]) {
    ///     assert_eq!(element, expected);
    /// }
    /// ```
    #[inline]
    pub fn iter(&self) -> Iter<T> {
        Iter::from(self)
    }

    /// Returns current size, be careful with race conditions when using it since other threads can change it right after the read
    ///
    /// `Relaxed` ordering is used to extract the length, so you shouldn't depend on this being sequentially consistent, only atomic
    ///
    /// ```rust
    /// # use hv_sync::monotonic_list::MonotonicList;
    /// let list = MonotonicList::new();
    /// list.push(3);
    /// list.push(2);
    /// assert_eq!(list.len(), 2);
    /// list.push(5);
    /// assert_eq!(list.len(), 3);
    /// ```
    /// Atomically extracts `Inner`'s size
    #[inline]
    pub fn len(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Checks if `VS` is currently empty, be careful with race conditions when using it since other threads can change it right after the read
    ///
    /// `Relaxed` ordering is used to extract the length, so you shouldn't depend on this being sequentially consistent, only atomic
    ///
    /// ```rust
    /// # use hv_sync::monotonic_list::MonotonicList;
    /// let list = MonotonicList::new();
    /// assert!(list.is_empty());
    /// list.push(());
    /// assert!(!list.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Append another [`MonotonicList`] to the end of this list, mutating it in-place.
    ///
    /// ```rust
    /// # use hv_sync::monotonic_list::MonotonicList;
    /// let list = MonotonicList::new();
    /// list.push(1);
    /// list.push(2);
    /// list.push(3);
    /// let list2 = MonotonicList::new();
    /// list2.push(4);
    /// list2.push(5);
    /// list2.push(6);
    /// list.append(list2);
    /// assert_eq!(list.len(), 6);
    /// assert_eq!(list.iter().collect::<Vec<_>>(), vec![&1, &2, &3, &4, &5, &6]);
    #[inline]
    pub fn append(&self, other: Self) {
        let (size, first, last) = other.into_inner();
        // We own `Inner<T>` so we can pass its ownership of its nodes to `push_chain`
        // And we don't drop them
        unsafe { self.push_chain(first, last, size) };
    }
}

impl<T: Debug> Debug for MonotonicList<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_tuple("MonotonicList")
            .field(&self.iter().collect::<Vec<_>>())
            .finish()
    }
}

impl<'a, T> Extend<T> for &'a MonotonicList<T> {
    #[inline]
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.append(MonotonicList::<T>::from_iter(iter));
    }
}

impl<T> Extend<T> for MonotonicList<T> {
    #[inline]
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        <&Self>::extend(&mut &*self, iter);
    }
}

impl<'a, T: 'a + Copy> Extend<&'a T> for MonotonicList<T> {
    #[inline]
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        Self::extend(self, iter.into_iter().cloned())
    }
}

impl<'a, T: 'a + Clone> FromIterator<&'a T> for MonotonicList<T> {
    #[inline]
    fn from_iter<I: IntoIterator<Item = &'a T>>(iter: I) -> Self {
        Self::from_iter(iter.into_iter().cloned())
    }
}

/// Lock-free iterator over a [`MonotonicList`].
///
/// ```rust
/// # use hv_sync::monotonic_list::MonotonicList;
/// let vs = MonotonicList::new();
/// vs.push(3);
/// vs.push(4);
/// vs.push(5);
/// let _ = vs.iter().map(|n| println!("Number: {}", n)).count();
/// ```
pub struct Iter<'a, T> {
    inner: &'a MonotonicList<T>,
    /// Current node in iteration
    current: Option<NonNull<Node<T>>>,
    /// Iteration index
    index: usize,
}

impl<'a, T> Clone for Iter<'a, T> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner,
            current: self.current,
            index: self.index,
        }
    }
}

impl<'a, T: Debug> Debug for Iter<'a, T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        // We can deref its pointer because `inner` owns it and we own `inner`
        let curr = self.current.as_ref().map(|ptr| unsafe { ptr.as_ref() });
        f.debug_struct("Iter")
            .field("inner", &self.inner)
            .field("current", &curr)
            .field("index", &self.index)
            .finish()
    }
}

impl<'a, T> From<&'a MonotonicList<T>> for Iter<'a, T> {
    #[inline]
    fn from(inner: &'a MonotonicList<T>) -> Self {
        Self {
            current: inner.first_node(),
            inner,
            index: 0,
        }
    }
}

impl<'a, T> Iter<'a, T> {
    /// Returns reference to last element in the list.
    ///
    /// `Relaxed` ordering is used to extract the `last_node`, so you shouldn't depend on this being
    /// sequentially consistent; only atomic.
    ///
    /// ```rust
    /// # use hv_sync::monotonic_list::MonotonicList;
    /// let vs = MonotonicList::new();
    /// vs.push(2);
    /// vs.push(3);
    /// vs.push(4);
    /// let iter = vs.iter();
    /// assert_eq!(iter.last_node(), Some(&4));
    /// ```
    #[inline]
    pub fn last_node(&self) -> Option<&'a T> {
        // We can deref its pointer because `inner` owns it and we own `inner`
        // We need to hack around the borrow checker to "prove" that
        // the ref extracted from `NonNull` has the same lifetime as `&self`
        self.inner
            .last_node()
            .map(|nn| unsafe { (*nn.as_ptr()).value() })
    }

    /// Returns current iterator size (may grow, be careful with race-conditions)
    ///
    /// If `Iter` was originally empty or was already consumed it will not grow (`FusedIterator`)
    ///
    /// `Relaxed` ordering is used to extract the length, so you shouldn't depend on this being sequentially consistent, only atomic
    ///
    /// ```rust
    /// # use hv_sync::monotonic_list::MonotonicList;
    /// let vs = MonotonicList::new();
    /// vs.push(3);
    /// let iter = vs.iter();
    /// assert_eq!(iter.len(), 1);
    ///
    /// vs.push(2);
    /// assert_eq!(iter.len(), 2);
    ///
    /// let mut iter2 = vs.iter();
    /// drop(iter2.next());
    /// drop(iter2.next());
    /// assert_eq!(iter2.next(), None);
    /// assert_eq!(iter2.len(), 0);
    ///
    /// vs.push(2);
    /// // `iter2` is fused
    /// assert_eq!(iter2.len(), 0);
    /// // But `iter` is not.
    /// assert_eq!(iter.len(), 3);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.current.map_or(0, |_| self.inner.len())
    }

    /// Checks if iterator's length is empty (if the iterator will return `None` on `next`.)
    ///
    /// `Relaxed` ordering is used to extract the length, so you shouldn't depend on this being
    /// sequentially consistent, only atomic.
    ///
    /// ```rust
    /// # use hv_sync::monotonic_list::MonotonicList;
    /// let vs = MonotonicList::new();
    /// vs.push(3);
    ///
    /// let mut iter = vs.iter();
    /// assert!(!iter.is_empty());
    ///
    /// // Consumes iterator to make it empty
    /// let _ = iter.by_ref().count();
    /// assert!(iter.is_empty());
    ///
    /// // Iterator is fused
    /// assert!(iter.is_empty());
    /// vs.push(2);
    /// assert!(iter.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.current.map_or(true, |_| self.len() == 0)
    }

    /// Obtains current iterator index
    ///
    /// ```rust
    /// # use hv_sync::monotonic_list::MonotonicList;
    /// let vs = MonotonicList::new();
    /// vs.push(3);
    /// vs.push(4);
    /// let mut iter = &mut vs.iter();
    ///
    /// assert_eq!(iter.next(), Some(&3));
    /// assert_eq!(iter.index(), 1);
    /// assert_eq!(iter.next(), Some(&4));
    /// assert_eq!(iter.index(), 2);
    ///
    /// // Index doesn't grow after iterator is consumed
    /// assert!(iter.next().is_none());
    /// assert_eq!(iter.index(), 2);
    /// ```
    #[inline]
    pub fn index(&self) -> usize {
        self.index
    }
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // We can deref its pointer because `inner` owns it and we own `inner`
        // We need to hack around the borrow checker to "prove" that
        // the ref extracted from `NonNull` has the same lifetime as `&self`
        let data = if let Some(ptr) = self.current {
            self.index += 1;
            Some(unsafe { (*ptr.as_ptr()).value() })
        } else {
            None
        };

        debug_assert!(
            self.is_empty() && self.index == 0 && data.is_none() || !self.inner.is_empty()
        );
        debug_assert!((self.index <= self.len() && data.is_some()) || self.index >= self.len());
        debug_assert!((self.index > self.len() && data.is_none()) || self.index <= self.len());

        // We can deref its pointer because `inner` owns it and we own `inner`
        // We need to hack around the borrow checker to "prove" that
        // the ref extracted from `NonNull` has the same lifetime as `&self`
        self.current = self
            .current
            .and_then(|n| unsafe { (*n.as_ptr()).next() })
            .and_then(|n| NonNull::new(n as *const Node<T> as *mut Node<T>));
        data
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.index, Some(self.len()))
    }
}

impl<'a, T> FusedIterator for Iter<'a, T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[macro_export]
    macro_rules! list {
        () => (MonotonicList::default());
        ($elem: expr; $n: expr) => {{
            let vs = MonotonicList::default();
            for _ in 0..$n {
                vs.push($elem);
            }
            vs
        }};
        ($($x: expr),+) => (list![$($x,)+]);
        ($($x: expr,)+) => {{
            let vs = MonotonicList::default();
            $(vs.push($x);)+
            vs
        }};
    }

    #[test]
    fn extend_partial_eq() {
        let vs: MonotonicList<u8> = list![1, 2, 3, 4, 5];
        let iter = &mut vs.iter();
        (&vs).extend(iter.cloned());
        assert_eq!(
            vs.iter().collect::<Vec<_>>(),
            vec![&1u8, &2, &3, &4, &5, &1, &2, &3, &4, &5]
        );
    }

    #[test]
    fn test_send() {
        fn assert_send<T: Send>() {}
        assert_send::<MonotonicList<()>>();
    }

    #[test]
    fn test_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<MonotonicList<()>>();
    }
}
