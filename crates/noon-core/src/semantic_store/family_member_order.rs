//! Derived order-statistic metadata for the store's existing family links.
//!
//! The intrusive list remains authoritative membership/order. This AVL index
//! stores only member identities, parent/child links and subtree sizes; no values,
//! second membership map or new identity allocator. All metadata edits accompany
//! validated list edits. Comparing distant aliases never walks unrelated siblings.

use super::{OrderedFamilyMembers, SemanticNodeId};

type Node = SemanticNodeId;

#[derive(Clone, Copy, Debug)]
pub(super) struct MemberOrderLink {
    left: Option<Node>,
    right: Option<Node>,
    parent: Option<Node>,
    size: usize,
    height: u16,
}

impl Default for MemberOrderLink {
    fn default() -> Self {
        Self {
            left: None,
            right: None,
            parent: None,
            size: 1,
            height: 1,
        }
    }
}

impl OrderedFamilyMembers {
    fn order_link(&self, node: Node) -> MemberOrderLink {
        #[cfg(test)]
        tests::READS.with(|reads| reads.set(reads.get() + 1));
        self.links[&node].order
    }

    fn order_size(&self, node: Option<Node>) -> usize {
        node.map_or(0, |node| self.order_link(node).size)
    }

    fn order_height(&self, node: Option<Node>) -> u16 {
        node.map_or(0, |node| self.order_link(node).height)
    }

    fn order_parent(&mut self, node: Option<Node>, parent: Option<Node>) {
        if let Some(node) = node {
            self.links.get_mut(&node).unwrap().order.parent = parent;
        }
    }

    fn order_children(&mut self, node: Node, left: Option<Node>, right: Option<Node>) {
        let size = 1 + self.order_size(left) + self.order_size(right);
        let height = 1 + self.order_height(left).max(self.order_height(right));
        let link = &mut self.links.get_mut(&node).unwrap().order;
        link.left = left;
        link.right = right;
        link.size = size;
        link.height = height;
        self.order_parent(left, Some(node));
        self.order_parent(right, Some(node));
    }

    fn order_rotate(&mut self, node: Node, left: bool) -> Node {
        let link = self.order_link(node);
        let pivot = if left { link.right } else { link.left }.unwrap();
        let child = self.order_link(pivot);
        if left {
            self.order_children(node, link.left, child.left);
            self.order_children(pivot, Some(node), child.right);
        } else {
            self.order_children(node, child.right, link.right);
            self.order_children(pivot, child.left, Some(node));
        }
        self.order_parent(Some(pivot), link.parent);
        pivot
    }

    fn order_balance(&mut self, node: Node) -> Node {
        let link = self.order_link(node);
        let left_height = self.order_height(link.left);
        let right_height = self.order_height(link.right);
        if left_height > right_height + 1 {
            let left = self.order_link(link.left.unwrap());
            if self.order_height(left.right) > self.order_height(left.left) {
                let rotated = self.order_rotate(link.left.unwrap(), true);
                self.order_children(node, Some(rotated), link.right);
            }
            self.order_rotate(node, false)
        } else if right_height > left_height + 1 {
            let right = self.order_link(link.right.unwrap());
            if self.order_height(right.left) > self.order_height(right.right) {
                let rotated = self.order_rotate(link.right.unwrap(), false);
                self.order_children(node, link.left, Some(rotated));
            }
            self.order_rotate(node, true)
        } else {
            node
        }
    }

    fn order_insert_at(&mut self, root: Option<Node>, node: Node, rank: usize) -> Node {
        let Some(root) = root else { return node };
        let link = self.order_link(root);
        let left_size = self.order_size(link.left);
        if rank <= left_size {
            let left = self.order_insert_at(link.left, node, rank);
            self.order_children(root, Some(left), link.right);
        } else {
            let right = self.order_insert_at(link.right, node, rank - left_size - 1);
            self.order_children(root, link.left, Some(right));
        }
        self.order_balance(root)
    }

    fn order_remove_at(&mut self, root: Node, rank: usize) -> Option<Node> {
        let link = self.order_link(root);
        let left_size = self.order_size(link.left);
        if rank < left_size {
            let left = self.order_remove_at(link.left.unwrap(), rank);
            self.order_children(root, left, link.right);
        } else if rank > left_size {
            let right = self.order_remove_at(link.right.unwrap(), rank - left_size - 1);
            self.order_children(root, link.left, right);
        } else {
            let (Some(left), Some(right)) = (link.left, link.right) else {
                let remaining = link.left.or(link.right);
                self.order_parent(remaining, link.parent);
                return remaining;
            };
            let mut successor = right;
            while let Some(left) = self.order_link(successor).left {
                successor = left;
            }
            let right = self.order_remove_at(right, 0);
            self.order_parent(Some(successor), link.parent);
            self.order_children(successor, Some(left), right);
            return Some(self.order_balance(successor));
        }
        Some(self.order_balance(root))
    }

    pub(super) fn order_rank(&self, mut node: Node) -> Option<usize> {
        if !self.links.contains_key(&node) {
            return None;
        }
        let mut link = self.order_link(node);
        let mut rank = self.order_size(link.left);
        while let Some(parent) = link.parent {
            let parent_link = self.order_link(parent);
            if parent_link.right == Some(node) {
                rank += 1 + self.order_size(parent_link.left);
            }
            node = parent;
            link = parent_link;
        }
        Some(rank)
    }

    pub(super) fn order_insert(&mut self, node: Node, before: Option<Node>) {
        let rank = before.map_or_else(
            || self.order_size(self.order_root),
            |before| {
                self.order_rank(before)
                    .expect("validated order anchor is a member")
            },
        );
        self.order_root = Some(self.order_insert_at(self.order_root, node, rank));
        self.order_parent(self.order_root, None);
    }

    pub(super) fn order_remove(&mut self, node: Node) {
        let rank = self
            .order_rank(node)
            .expect("removed order node is a member");
        self.order_root = self.order_remove_at(self.order_root.unwrap(), rank);
        self.order_parent(self.order_root, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    std::thread_local! {
        pub(super) static READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    fn verify(members: &OrderedFamilyMembers, expected: &[Node]) {
        fn walk(
            members: &OrderedFamilyMembers,
            id: Option<Node>,
            parent: Option<Node>,
            out: &mut Vec<Node>,
        ) -> (usize, u16) {
            let Some(id) = id else { return (0, 0) };
            let link = members.order_link(id);
            assert_eq!(link.parent, parent);
            let (lc, lh) = walk(members, link.left, Some(id), out);
            out.push(id);
            let (rc, rh) = walk(members, link.right, Some(id), out);
            assert!(lh.abs_diff(rh) <= 1);
            assert_eq!(link.size, lc + rc + 1);
            assert_eq!(link.height, lh.max(rh) + 1);
            (link.size, link.height)
        }
        let mut ordered = Vec::new();
        walk(members, members.order_root, None, &mut ordered);
        assert_eq!(ordered, expected);
        assert_eq!(members.iter().collect::<Vec<_>>(), expected);
        for (index, id) in expected.iter().enumerate() {
            assert_eq!(members.order_rank(*id), Some(index));
        }
    }

    #[test]
    fn family_order_index_matches_list_after_add_remove_reorder_and_generation_reuse() {
        let mut members = OrderedFamilyMembers::default();
        let mut expected = Vec::new();
        let mut seed = 9181u64;
        for step in 0..10_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let node = Node::new((seed % 96) as u32, (step / 2000) as u32);
            match (seed >> 16) % 3 {
                0 => {
                    if members.push(node) {
                        expected.push(node);
                    }
                }
                1 => {
                    members.remove(node);
                    expected.retain(|id| *id != node);
                }
                _ if expected.contains(&node) => {
                    let at = (seed as usize >> 24) % (expected.len() + 1);
                    let anchor = expected.get(at).copied();
                    members.move_before(node, anchor);
                    if anchor != Some(node) {
                        expected.retain(|id| *id != node);
                        let at = anchor.map_or(expected.len(), |anchor| {
                            expected.iter().position(|id| *id == anchor).unwrap()
                        });
                        expected.insert(at, node);
                    }
                }
                _ => {}
            }
            verify(&members, &expected);
        }
        for node in expected.clone() {
            assert!(members.remove(node));
            expected.retain(|id| *id != node);
            verify(&members, &expected);
        }
    }

    #[test]
    fn distant_family_member_comparison_and_edits_are_logarithmic() {
        for count in [16, 65_536] {
            let mut members = OrderedFamilyMembers::default();
            for slot in 0..count {
                members.push(Node::new(slot, 0));
            }
            let first = Node::new(0, 0);
            let last = Node::new(count - 1, 0);
            READS.with(|reads| reads.set(0));
            assert_eq!(members.order_rank(first), Some(0));
            assert_eq!(members.order_rank(last), Some(count as usize - 1));
            let compare_reads = READS.with(|reads| reads.get());
            assert!(compare_reads <= 4 * (count.ilog2() as usize + 1));
            READS.with(|reads| reads.set(0));
            members.move_before(last, Some(first));
            let edit_reads = READS.with(|reads| reads.get());
            assert!(edit_reads <= 50 * (count.ilog2() as usize + 1));
            let mut expected = vec![last];
            expected.extend((0..count - 1).map(|slot| Node::new(slot, 0)));
            verify(&members, &expected);
            println!("family size={count}: compare reads={compare_reads}, move reads={edit_reads}");
        }
    }

    #[test]
    fn rank_metadata_does_not_make_semantic_equality_depend_on_edit_history() {
        let mut first = OrderedFamilyMembers::default();
        let mut second = OrderedFamilyMembers::default();
        let nodes = (0..40).map(|slot| Node::new(slot, 0)).collect::<Vec<_>>();
        for &node in &nodes {
            first.push(node);
        }
        for &node in nodes.iter().rev() {
            second.push(node);
        }
        for &node in &nodes {
            second.move_before(node, None);
        }
        verify(&first, &nodes);
        verify(&second, &nodes);
        assert_eq!(first, second);
    }
}
