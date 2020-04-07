use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;

/// Bidirectional map where every L must map to exactly one R, but every R can map to multiple L.
pub struct MultiBiMap<L: Clone + Eq + Hash, R: Clone + Eq + Hash> {
    left: HashMap<L, R>,
    right: HashMap<R, Vec<L>>,
}

impl<L: Clone + Eq + Hash, R: Clone + Eq + Hash> MultiBiMap<L, R> {
    /// Create a new bidirectional map
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the map is empty
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }

    /// Returns the number of pairs in the map.
    /// This is equivalent to the number of different `L` elements.
    pub fn len(&self) -> usize {
        self.left.len()
    }

    /// Returns true if the map contains `L`
    pub fn contains_left(&self, l: &L) -> bool {
        self.left.contains_key(l)
    }

    /// Returns true if the map contains `R`
    pub fn contains_right(&self, r: &R) -> bool {
        self.right.contains_key(r)
    }

    /// Get the one `R` paired with `L`
    pub fn get_by_left(&self, l: &L) -> Option<&R> {
        self.left.get(l)
    }

    /// Get all the `L` paired with this `R`
    pub fn get_by_right(&self, r: &R) -> &[L] {
        self.right.get(r).map(|vec| vec.as_slice()).unwrap_or(&[])
    }

    /// Insert a new `(L, R)` pair.
    /// Since `L` must be unique, any old mapping using this value of `L` will be removed.
    pub fn insert(&mut self, l: L, r: R) {
        self.remove_by_left(&l);
        self.left.insert(l.clone(), r.clone());
        self.right.entry(r).or_default().push(l);
    }

    /// Remove `L` and return the `(L, R)` pair
    pub fn remove_by_left(&mut self, l: &L) -> Option<(L, R)> {
        self.left.remove(l).map(|r| {
            let v = self.right.get_mut(&r).unwrap();
            let pos = v.iter().position(|x| x == l).unwrap();
            v.swap_remove(pos);

            (l.clone(), r)
        })
    }

    /// Remove `R` and return all the `(L, R)` pairs mapping to this `R`, as a `(Vec<L>, R)`.
    pub fn remove_by_right(&mut self, r: &R) -> Option<(Vec<L>, R)> {
        self.right.remove(r).map(|l| {
            for x in &l {
                self.left.remove(x);
            }

            (l, r.clone())
        })
    }

    /// Iterate over all the `(L, R)` pairs
    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, L, R> {
        self.left.iter()
    }
}

impl<L: Clone + Eq + Hash, R: Clone + Eq + Hash> Default for MultiBiMap<L, R> {
    fn default() -> Self {
        Self {
            left: HashMap::default(),
            right: HashMap::default(),
        }
    }
}

impl<L: Clone + fmt::Debug + Eq + Hash, R: Clone + fmt::Debug + Eq + Hash> fmt::Debug
    for MultiBiMap<L, R>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.left.iter()).finish()
    }
}
