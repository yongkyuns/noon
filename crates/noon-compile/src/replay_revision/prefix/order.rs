//! Sparse reverse ordering overlay; it owns no execution or semantic state.
use super::CompiledScene;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Links {
    previous: Option<u32>,
    next: Option<u32>,
}

pub(super) struct SparseOrder<'a> {
    scene: &'a CompiledScene,
    changed: BTreeMap<u32, Option<Links>>,
    tail: Option<u32>,
}
impl<'a> SparseOrder<'a> {
    pub(super) fn new(scene: &'a CompiledScene) -> Self {
        Self {
            scene,
            changed: BTreeMap::new(),
            tail: scene.family_order.last().copied(),
        }
    }
    fn links(&mut self, index: u32) -> Option<Links> {
        *self.changed.entry(index).or_insert_with(|| {
            let rank = self.scene.family_rank(index)? as usize;
            Some(Links {
                previous: rank
                    .checked_sub(1)
                    .and_then(|rank| self.scene.family_order.get(rank).copied()),
                next: self.scene.family_order.get(rank + 1).copied(),
            })
        })
    }
    pub(super) fn successor(&mut self, index: u32) -> Option<u32> {
        self.links(index).and_then(|links| links.next)
    }
    pub(super) fn restore(&mut self, index: u32, live: bool, before: Option<u32>) -> Option<()> {
        if let Some(old) = self.links(index) {
            if let Some(previous) = old.previous {
                let mut links = self.links(previous)?;
                links.next = old.next;
                self.changed.insert(previous, Some(links));
            }
            if let Some(next) = old.next {
                let mut links = self.links(next)?;
                links.previous = old.previous;
                self.changed.insert(next, Some(links));
            } else {
                self.tail = old.previous;
            }
        }
        self.changed.insert(index, None);
        if live {
            // Match ordinary replay: an unavailable successor denotes the tail.
            let next = before.filter(|&anchor| self.links(anchor).is_some());
            let previous = match next {
                Some(next) => self.links(next)?.previous,
                None => self.tail,
            };
            if let Some(previous) = previous {
                let mut links = self.links(previous)?;
                links.next = Some(index);
                self.changed.insert(previous, Some(links));
            }
            if let Some(next) = next {
                let mut links = self.links(next)?;
                links.previous = Some(index);
                self.changed.insert(next, Some(links));
            } else {
                self.tail = Some(index);
            }
            self.changed.insert(index, Some(Links { previous, next }));
        }
        Some(())
    }
}
