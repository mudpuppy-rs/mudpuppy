use std::collections::hash_map::{self, HashMap};
use std::fmt::{Debug, Display};

use tracing::trace;

/// A map of identifiable objects.
///
/// Handles the generation of unique identifiers for objects, and provides a way to
/// store and retrieve them.
#[derive(Debug)]
pub struct IdMap<Value> {
    map: HashMap<u32, Value>,
    next_id: u32,
}

impl<Value> IdMap<Value>
where
    Value: Identifiable + Debug + Display,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next_id: 0,
        }
    }

    #[must_use]
    pub fn ids(&self) -> Vec<u32> {
        self.map.keys().copied().collect()
    }

    pub fn construct(&mut self, builder: impl FnOnce(u32) -> Value) -> u32 {
        let id = self.next_id;
        self.next_id = id + 1;
        let value = builder(id);
        trace!("constructed {}", value);
        self.insert(value);
        id
    }

    pub fn insert(&mut self, value: Value) {
        self.map.insert(value.id(), value);
    }

    pub fn remove(&mut self, id: u32) {
        self.map.remove(&id);
    }

    pub fn remove_value(&mut self, id: &Value) {
        self.map.remove(&id.id());
    }

    #[must_use]
    pub fn get(&self, id: u32) -> Option<&Value> {
        self.map.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Value> {
        self.map.get_mut(&id)
    }

    #[must_use]
    pub fn iter(&self) -> hash_map::Iter<u32, Value> {
        <&Self as IntoIterator>::into_iter(self)
    }

    #[must_use]
    pub fn iter_mut(&mut self) -> hash_map::IterMut<u32, Value> {
        <&mut Self as IntoIterator>::into_iter(self)
    }

    pub fn values_mut(&mut self) -> hash_map::ValuesMut<u32, Value> {
        self.map.values_mut()
    }

    #[must_use]
    pub fn as_map(&self) -> &HashMap<u32, Value> {
        &self.map
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl<Value> Default for IdMap<Value>
where
    Value: Identifiable + Debug + Display,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, Value> IntoIterator for &'a IdMap<Value> {
    type Item = (&'a u32, &'a Value);
    type IntoIter = hash_map::Iter<'a, u32, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}

impl<'a, Value> IntoIterator for &'a mut IdMap<Value> {
    type Item = (&'a u32, &'a mut Value);
    type IntoIter = hash_map::IterMut<'a, u32, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.map.iter_mut()
    }
}
pub trait Identifiable {
    fn id(&self) -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Mud, SessionInfo};

    #[test]
    fn session_id_map() {
        let mut map: IdMap<SessionInfo> = IdMap::new();

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
