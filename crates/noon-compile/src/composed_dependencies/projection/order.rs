//! Transaction-local permutation overlay. Copy only genuinely shifted positions.
use super::*;

pub(super) struct ProjectedOrder<'a> {
    scene: &'a CompiledScene,
    family: bool,
    len: usize,
    positions: BTreeMap<usize, ObjectId>,
    ranks: BTreeMap<ObjectId, Option<usize>>,
}
impl<'a> ProjectedOrder<'a> {
    pub(super) fn new(scene: &'a CompiledScene, family: bool) -> Self {
        Self {
            scene,
            family,
            len: if family {
                scene.family_order.len()
            } else {
                scene.painter_order.len()
            },
            positions: BTreeMap::new(),
            ranks: BTreeMap::new(),
        }
    }
    pub(super) fn rank(&self, id: ObjectId) -> Option<usize> {
        if let Some(rank) = self.ranks.get(&id) {
            return *rank;
        }
        let index = self.scene.object_index(id)?;
        if self.family {
            self.scene.family_rank(index)
        } else {
            self.scene.painter_rank(index)
        }
        .map(|rank| rank as usize)
    }
    fn at(&self, rank: usize) -> ObjectId {
        assert!(rank < self.len);
        self.positions.get(&rank).copied().unwrap_or_else(|| {
            let order = if self.family {
                &self.scene.family_order
            } else {
                &self.scene.painter_order
            };
            self.scene.objects[order[rank] as usize].id
        })
    }
    fn put(&mut self, rank: usize, id: ObjectId) {
        self.positions.insert(rank, id);
        self.ranks.insert(id, Some(rank));
    }
    pub(super) fn append(&mut self, id: ObjectId) {
        self.put(self.len, id);
        self.len += 1;
    }
    pub(super) fn remove(&mut self, id: ObjectId) {
        let Some(position) = self.rank(id) else {
            return;
        };
        for rank in position..self.len - 1 {
            let next = self.at(rank + 1);
            self.put(rank, next);
        }
        self.len -= 1;
        self.positions.remove(&self.len);
        self.ranks.insert(id, None);
    }
    pub(super) fn move_to(&mut self, id: ObjectId, destination: usize) {
        let Some(position) = self.rank(id) else {
            return;
        };
        if position < destination {
            for rank in position..destination {
                let next = self.at(rank + 1);
                self.put(rank, next);
            }
        } else {
            for rank in (destination + 1..=position).rev() {
                let next = self.at(rank - 1);
                self.put(rank, next);
            }
        }
        if position != destination {
            self.put(destination, id);
        }
    }
    pub(super) fn reposition(
        &mut self,
        id: ObjectId,
        compare: impl Fn(ObjectId, ObjectId) -> std::cmp::Ordering,
    ) {
        let Some(position) = self.rank(id) else {
            return;
        };
        let (mut low, mut high) = (0, self.len - 1);
        while low < high {
            let middle = low + (high - low) / 2;
            let candidate = self.at(middle + usize::from(middle >= position));
            if compare(candidate, id) == std::cmp::Ordering::Less {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        self.move_to(id, low);
    }
    pub(super) fn tail(&self) -> usize {
        self.len.saturating_sub(1)
    }
    pub(super) fn touched(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.ranks.keys().copied()
    }
    pub(super) fn staged_len(&self) -> usize {
        self.positions.len()
    }
}
