use std::collections::hash_map::{self, HashMap};
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::ops::Add;

use tracing::trace;

/// A map of identifiable objects.
///
/// Handles the generation of unique identifiers for objects, and provides a way to
/// store and retrieve them.
#[derive(Debug)]
pub struct IdMap<Key, Value> {
    map: HashMap<Key, Value>,
    next_id: Key,
}

impl<Key, Value> IdMap<Key, Value>
where
    Key: IncrementableId,
    Value: Identifiable<Key> + Debug + Display,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next_id: Key::default(),
        }
    }

    #[must_use]
    pub fn ids(&self) -> Vec<Key> {
        self.map.keys().copied().collect()
    }

    pub fn construct(&mut self, builder: impl FnOnce(Key) -> Value) -> Key {
        let id = self.next_id;
        self.next_id = id + Key::from(1);
        let value = builder(id);
        trace!("constructed {}", value);
        self.insert(value);
        id
    }

    pub fn insert(&mut self, value: Value) {
        self.map.insert(value.id(), value);
    }

    pub fn remove(&mut self, id: Key) {
        self.map.remove(&id);
    }

    pub fn remove_value(&mut self, id: &Value) {
        self.map.remove(&id.id());
    }

    #[must_use]
    pub fn get(&self, id: Key) -> Option<&Value> {
        self.map.get(&id)
    }

    pub fn get_mut(&mut self, id: Key) -> Option<&mut Value> {
        self.map.get_mut(&id)
    }

    #[must_use]
    pub fn iter(&self) -> hash_map::Iter<Key, Value> {
        <&Self as IntoIterator>::into_iter(self)
    }

    #[must_use]
    pub fn iter_mut(&mut self) -> hash_map::IterMut<Key, Value> {
        <&mut Self as IntoIterator>::into_iter(self)
    }

    pub fn values_mut(&mut self) -> hash_map::ValuesMut<Key, Value> {
        self.map.values_mut()
    }

    #[must_use]
    pub fn as_map(&self) -> &HashMap<Key, Value> {
        &self.map
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl<Key, Value> Default for IdMap<Key, Value>
where
    Key: IncrementableId,
    Value: Identifiable<Key> + Debug + Display,
{
    #[must_use]
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, Key, Value> IntoIterator for &'a IdMap<Key, Value> {
    type Item = (&'a Key, &'a Value);
    type IntoIter = hash_map::Iter<'a, Key, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}

impl<'a, Key, Value> IntoIterator for &'a mut IdMap<Key, Value> {
    type Item = (&'a Key, &'a mut Value);
    type IntoIter = hash_map::IterMut<'a, Key, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.map.iter_mut()
    }
}
pub trait Identifiable<Key> {
    fn id(&self) -> Key;
}

pub trait IncrementableId: Copy + Eq + Hash + Default + From<u8> + Add<Output = Self> {}

// Blank implementation for all types that meet our reqs. Notably this includes the usual numeric
// types, u32, etc.
impl<T> IncrementableId for T where T: Copy + Eq + Hash + Default + From<u8> + Add<Output = Self> {}

/// Create a strongly typed wrapper around a numeric type.
///
/// The generated wrapper:
/// * Is transparent, adding no additional overhead.
/// * Is strongly typed, so it can't be confused for an ID in a different domain.
/// * Derives traits required for use with an `IdMap`.
/// * Can be sent across the FFI boundary with Python code.
/// * Impls `Deref` and `DerefMut` to the underlying numeric type for ease of use.
macro_rules! numeric_id {
    ($name:ident, $ty:ty) => {
        #[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
        #[repr(transparent)]
        #[pyclass]
        pub struct $name($ty);

        #[pymethods]
        #[allow(clippy::trivially_copy_pass_by_ref)] // Can't move `self` for __str__ and __repr__.
        impl $name {
            #[new]
            fn new(val: $ty) -> Self {
                Self(val)
            }

            fn __repr__(&self) -> String {
                format!("{self:?}")
            }

            fn __str__(&self) -> String {
                format!("{}", self.0)
            }

            fn __hash__(&self) -> u64 {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                self.hash(&mut hasher);
                hasher.finish()
            }

            fn __eq__(&self, other: &Self) -> bool {
                self.0 == other.0
            }

            fn __lt__(&self, other: &Self) -> bool {
                self.0 < other.0
            }

            fn __le__(&self, other: &Self) -> bool {
                self.0 <= other.0
            }

            fn __gt__(&self, other: &Self) -> bool {
                self.0 > other.0
            }

            fn __ge__(&self, other: &Self) -> bool {
                self.0 >= other.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<u8> for $name {
            fn from(value: u8) -> Self {
                Self(<$ty>::from(value))
            }
        }

        impl std::ops::Add for $name {
            type Output = Self;

            fn add(self, rhs: Self) -> Self::Output {
                Self(self.0 + rhs.0)
            }
        }

        impl std::ops::Deref for $name {
            type Target = $ty;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl std::ops::DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }
    };
}

pub(crate) use numeric_id;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Mud, SessionId, SessionInfo};

    #[test]
    fn session_id_map() {
        let mut map: IdMap<SessionId, SessionInfo> = IdMap::new();

        let mud = Mud::default();
        let first_id = map.construct(|id| SessionInfo {
            id,
            mud_name: mud.name.clone(),
        });

        let second_id = map.construct(|id| SessionInfo {
            id,
            mud_name: mud.name.clone(),
        });
        assert_ne!(first_id, second_id);

        let mut ids = map.ids();
        ids.sort_unstable();
        assert_eq!(ids, vec![first_id, second_id]);

        for (id, info) in &map {
            assert_eq!(*id, info.id);
            println!("{}: {}", id, info.mud_name);
        }
    }
}
