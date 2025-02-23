use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use si_events::ContentHash;
use ulid::Ulid;

use super::NodeWeightError;
use crate::change_set::ChangeSet;
use crate::workspace_snapshot::vector_clock::VectorClockId;
use crate::workspace_snapshot::{node_weight::NodeWeightResult, vector_clock::VectorClock};

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct OrderingNodeWeight {
    id: Ulid,
    lineage_id: Ulid,
    /// The `id` of the items, in the order that they should appear in the container.
    order: Vec<Ulid>,
    content_hash: ContentHash,
    merkle_tree_hash: ContentHash,
    vector_clock_first_seen: VectorClock,
    vector_clock_recently_seen: VectorClock,
    vector_clock_write: VectorClock,
}

impl OrderingNodeWeight {
    pub fn content_hash(&self) -> ContentHash {
        self.content_hash
    }

    pub fn id(&self) -> Ulid {
        self.id
    }

    pub fn increment_seen_vector_clock(&mut self, change_set: &ChangeSet) -> NodeWeightResult<()> {
        self.vector_clock_first_seen
            .inc(change_set.vector_clock_id())?;

        Ok(())
    }

    pub fn increment_vector_clock(&mut self, change_set: &ChangeSet) -> NodeWeightResult<()> {
        self.vector_clock_write
            .inc(change_set.vector_clock_id())
            .map_err(Into::into)
    }

    pub fn lineage_id(&self) -> Ulid {
        self.lineage_id
    }

    pub fn mark_seen_at(&mut self, vector_clock_id: VectorClockId, seen_at: DateTime<Utc>) {
        self.vector_clock_recently_seen
            .inc_to(vector_clock_id, seen_at);
        if self
            .vector_clock_first_seen
            .entry_for(vector_clock_id)
            .is_none()
        {
            self.vector_clock_first_seen
                .inc_to(vector_clock_id, seen_at);
        }
    }

    pub fn merge_clocks(
        &mut self,
        change_set: &ChangeSet,
        other: &OrderingNodeWeight,
    ) -> NodeWeightResult<()> {
        self.vector_clock_write
            .merge(change_set.vector_clock_id(), other.vector_clock_write())?;
        self.vector_clock_first_seen.merge(
            change_set.vector_clock_id(),
            other.vector_clock_first_seen(),
        )?;

        Ok(())
    }

    pub fn merkle_tree_hash(&self) -> ContentHash {
        self.merkle_tree_hash
    }

    pub fn new(change_set: &ChangeSet) -> NodeWeightResult<Self> {
        Ok(Self {
            id: change_set.generate_ulid()?,
            lineage_id: change_set.generate_ulid()?,
            vector_clock_write: VectorClock::new(change_set.vector_clock_id())?,
            vector_clock_first_seen: VectorClock::new(change_set.vector_clock_id())?,
            ..Default::default()
        })
    }

    pub fn new_with_incremented_vector_clock(
        &self,
        change_set: &ChangeSet,
    ) -> NodeWeightResult<Self> {
        let mut new_ordering_weight = self.clone();
        new_ordering_weight.increment_vector_clock(change_set)?;

        Ok(new_ordering_weight)
    }

    pub fn node_hash(&self) -> ContentHash {
        self.content_hash()
    }

    pub fn order(&self) -> &Vec<Ulid> {
        &self.order
    }

    pub fn set_merkle_tree_hash(&mut self, new_hash: ContentHash) {
        self.merkle_tree_hash = new_hash;
    }

    pub fn set_order(&mut self, change_set: &ChangeSet, order: Vec<Ulid>) -> NodeWeightResult<()> {
        self.order = order;
        self.update_content_hash();
        self.increment_vector_clock(change_set)?;

        Ok(())
    }

    pub fn set_vector_clock_recently_seen_to(
        &mut self,
        change_set: &ChangeSet,
        new_val: DateTime<Utc>,
    ) {
        self.vector_clock_recently_seen
            .inc_to(change_set.vector_clock_id(), new_val);
    }

    fn update_content_hash(&mut self) {
        let mut content_hasher = ContentHash::hasher();
        let concat_elements = self
            .order
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<String>>()
            .join(" ");
        let content_bytes = concat_elements.as_bytes();
        content_hasher.update(content_bytes);

        self.content_hash = content_hasher.finalize();
    }

    pub fn vector_clock_first_seen(&self) -> &VectorClock {
        &self.vector_clock_first_seen
    }

    pub fn vector_clock_recently_seen(&self) -> &VectorClock {
        &self.vector_clock_recently_seen
    }

    pub fn vector_clock_write(&self) -> &VectorClock {
        &self.vector_clock_write
    }

    pub fn push_to_order(&mut self, change_set: &ChangeSet, id: Ulid) -> NodeWeightResult<()> {
        let mut order = self.order().to_owned();
        order.push(id);
        self.set_order(change_set, order)
    }

    pub fn remove_from_order(
        &mut self,
        change_set: &ChangeSet,
        id: Ulid,
    ) -> NodeWeightResult<bool> {
        let mut order = self.order.to_owned();
        order.retain(|&item_id| item_id != id);
        if order.len() != self.order().len() {
            self.set_order(change_set, order)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub fn get_index_for_id(&self, id: Ulid) -> NodeWeightResult<i64> {
        let order = self.order.to_owned();
        let index = order
            .iter()
            .position(|&key| key == id)
            .ok_or(NodeWeightError::MissingKeytForChildEntry(id))?;
        let ret: i64 = index.try_into().map_err(NodeWeightError::TryFromIntError)?;
        Ok(ret)
    }
}

impl std::fmt::Debug for OrderingNodeWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("OrderingNodeWeight")
            .field("id", &self.id.to_string())
            .field("lineage_id", &self.lineage_id.to_string())
            .field(
                "order",
                &self
                    .order
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<String>>(),
            )
            .field("content_hash", &self.content_hash)
            .field("merkle_tree_hash", &self.merkle_tree_hash)
            .field("vector_clock_first_seen", &self.vector_clock_first_seen)
            .field(
                "vector_clock_recently_seen",
                &self.vector_clock_recently_seen,
            )
            .field("vector_clock_write", &self.vector_clock_write)
            .finish()
    }
}
