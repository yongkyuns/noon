from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"{path}: expected one anchor, found {text.count(old)}\nANCHOR:\n{old[:500]}")
    p.write_text(text.replace(old, new, 1))

# Runtime exposes the live dense frame slots only at the transport boundary. The
# runtime itself continues to preserve tombstoned slot positions.
replace_once(
    "crates/noon-runtime/src/execution_slots.rs",
    """    pub fn live_object_count(&self) -> usize {
        self.inner.compiled.live_object_count()
    }

    pub fn preflight_transaction(
""",
    """    pub fn live_object_count(&self) -> usize {
        self.inner.compiled.live_object_count()
    }

    pub fn live_frame_indices(&self) -> Vec<usize> {
        self.inner
            .compiled
            .objects()
            .iter()
            .enumerate()
            .filter_map(|(index, object)| object.live.then_some(index))
            .collect()
    }

    pub fn preflight_transaction(
""",
)

replace_once(
    "crates/noon-web/src/legacy.rs",
    """    pub fn object_count(&self) -> usize {
        self.instance.live_object_count()
    }
}
""",
    """    pub fn object_count(&self) -> usize {
        self.instance.live_object_count()
    }

    pub(crate) fn live_frame_indices(&self) -> Vec<usize> {
        self.instance.live_frame_indices()
    }
}
""",
)

# Transport snapshots are compact live-object views. This keeps the existing
# snapshot protocol correct even though the engine frame preserves retired slots.
replace_once(
    "crates/noon-web/src/execution_transport.rs",
    """    previous_order: Vec<ObjectId>,
}
""",
    """    previous_order: Vec<ObjectId>,
    object_orders: HashMap<ObjectId, u32>,
}
""",
)
replace_once(
    "crates/noon-web/src/execution_transport.rs",
    """            previous_order: Vec::new(),
        }
""",
    """            previous_order: Vec::new(),
            object_orders: HashMap::new(),
        }
""",
)

old_encode = """    pub fn encode(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
    ) -> Result<Option<ExecutionDeltaEnvelope>, ExecutionTransportError> {
        if !frame.time.is_finite() {
            return Err(ExecutionTransportError::InvalidTime(frame.time));
        }

        let structural = self.sync_slots(frame)?;
        let snapshot = !self.initialized || structural || changes.is_all();
        if !snapshot && changes.is_empty() {
            return Ok(None);
        }

        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(ExecutionTransportError::SequenceExhausted)?;
        let indices = if snapshot {
            (0..frame.objects.len()).collect::<Vec<_>>()
        } else {
            changes.object_indices().to_vec()
        };
        let mut objects = Vec::with_capacity(indices.len());
        for index in indices {
            let order =
                u32::try_from(index).map_err(|_| ExecutionTransportError::SlotSpaceExhausted)?;
            objects.push(self.object_state(frame, index, order)?);
        }
        self.initialized = true;

        Ok(Some(ExecutionDeltaEnvelope {
            channel: EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: EXECUTION_TRANSPORT_VERSION,
            session: self.session,
            sequence,
            snapshot,
            time: frame.time,
            removed: Vec::new(),
            objects,
        }))
    }

    pub fn encode_json(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
    ) -> Result<Option<String>, ExecutionTransportError> {
        self.encode(frame, changes)?
            .map(|delta| serde_json::to_string(&delta).map_err(ExecutionTransportError::from))
            .transpose()
    }

    fn sync_slots(&mut self, frame: &FrameState) -> Result<bool, ExecutionTransportError> {
        let order = frame
            .objects
            .iter()
            .map(|object| object.id)
            .collect::<Vec<_>>();
"""
new_encode = """    pub fn encode(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
    ) -> Result<Option<ExecutionDeltaEnvelope>, ExecutionTransportError> {
        let live_indices = (0..frame.objects.len()).collect::<Vec<_>>();
        self.encode_live(frame, changes, &live_indices)
    }

    pub fn encode_live(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        live_indices: &[usize],
    ) -> Result<Option<ExecutionDeltaEnvelope>, ExecutionTransportError> {
        if !frame.time.is_finite() {
            return Err(ExecutionTransportError::InvalidTime(frame.time));
        }

        let structural = self.sync_slots(frame, live_indices)?;
        let snapshot = !self.initialized || structural || changes.is_all();
        if !snapshot && changes.is_empty() {
            return Ok(None);
        }

        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(ExecutionTransportError::SequenceExhausted)?;
        let indices = if snapshot {
            live_indices.to_vec()
        } else {
            changes.object_indices().to_vec()
        };
        let mut objects = Vec::with_capacity(indices.len());
        for (snapshot_order, index) in indices.into_iter().enumerate() {
            let object = frame
                .objects
                .get(index)
                .ok_or(ExecutionTransportError::SlotSpaceExhausted)?;
            let order = if snapshot {
                u32::try_from(snapshot_order)
                    .map_err(|_| ExecutionTransportError::SlotSpaceExhausted)?
            } else {
                *self
                    .object_orders
                    .get(&object.id)
                    .ok_or_else(|| ExecutionTransportError::UnknownSlot(
                        self.slots
                            .slot_for_object(object.id)
                            .map(TransportSlotId::from)
                            .unwrap_or(TransportSlotId { slot: u32::MAX, generation: 0 }),
                    ))?
            };
            objects.push(self.object_state(frame, index, order)?);
        }
        self.initialized = true;

        Ok(Some(ExecutionDeltaEnvelope {
            channel: EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: EXECUTION_TRANSPORT_VERSION,
            session: self.session,
            sequence,
            snapshot,
            time: frame.time,
            removed: Vec::new(),
            objects,
        }))
    }

    pub fn encode_json(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
    ) -> Result<Option<String>, ExecutionTransportError> {
        self.encode(frame, changes)?
            .map(|delta| serde_json::to_string(&delta).map_err(ExecutionTransportError::from))
            .transpose()
    }

    pub fn encode_live_json(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        live_indices: &[usize],
    ) -> Result<Option<String>, ExecutionTransportError> {
        self.encode_live(frame, changes, live_indices)?
            .map(|delta| serde_json::to_string(&delta).map_err(ExecutionTransportError::from))
            .transpose()
    }

    fn sync_slots(
        &mut self,
        frame: &FrameState,
        live_indices: &[usize],
    ) -> Result<bool, ExecutionTransportError> {
        let order = live_indices
            .iter()
            .map(|index| {
                frame
                    .objects
                    .get(*index)
                    .map(|object| object.id)
                    .ok_or(ExecutionTransportError::SlotSpaceExhausted)
            })
            .collect::<Result<Vec<_>, _>>()?;
"""
replace_once("crates/noon-web/src/execution_transport.rs", old_encode, new_encode)

replace_once(
    "crates/noon-web/src/execution_transport.rs",
    """        self.previous_order = order;
        Ok(structural)
    }
""",
    """        self.previous_order = order;
        self.object_orders.clear();
        for (order, object) in self.previous_order.iter().copied().enumerate() {
            let order = u32::try_from(order)
                .map_err(|_| ExecutionTransportError::SlotSpaceExhausted)?;
            self.object_orders.insert(object, order);
        }
        Ok(structural)
    }
""",
)

# Engine paths always provide the authoritative live slot list. This preserves
# compact transport order after a local runtime tombstone without compacting the
# runtime frame itself.
replace_once(
    "crates/noon-web/src/execution_transport.rs",
    """    pub fn initial_delta_json(&mut self) -> Result<String, ExecutionTransportError> {
        let changes = self.player.take_frame_changes();
        self.encoder
            .encode_json(self.player.frame(), &changes)?
            .ok_or(ExecutionTransportError::StructuralDeltaRequiresSnapshot)
    }
""",
    """    pub fn initial_delta_json(&mut self) -> Result<String, ExecutionTransportError> {
        let changes = self.player.take_frame_changes();
        let live_indices = self.player.live_frame_indices();
        self.encoder
            .encode_live_json(self.player.frame(), &changes, &live_indices)?
            .ok_or(ExecutionTransportError::StructuralDeltaRequiresSnapshot)
    }
""",
)
replace_once(
    "crates/noon-web/src/execution_transport.rs",
    """    fn take_delta_json(&mut self) -> Result<Option<String>, ExecutionTransportError> {
        let changes = self.player.take_frame_changes();
        self.encoder.encode_json(self.player.frame(), &changes)
    }

    fn force_snapshot_json(&mut self) -> Result<String, ExecutionTransportError> {
        self.encoder
            .encode_json(self.player.frame(), &FrameChanges::all())?
            .ok_or(ExecutionTransportError::StructuralDeltaRequiresSnapshot)
    }
""",
    """    fn take_delta_json(&mut self) -> Result<Option<String>, ExecutionTransportError> {
        let changes = self.player.take_frame_changes();
        let live_indices = self.player.live_frame_indices();
        self.encoder
            .encode_live_json(self.player.frame(), &changes, &live_indices)
    }

    fn force_snapshot_json(&mut self) -> Result<String, ExecutionTransportError> {
        let live_indices = self.player.live_frame_indices();
        self.encoder
            .encode_live_json(self.player.frame(), &FrameChanges::all(), &live_indices)?
            .ok_or(ExecutionTransportError::StructuralDeltaRequiresSnapshot)
    }
""",
)

replace_once(
    "crates/noon-web/src/execution_transport.rs",
    """    fn encode_current(
        encoder: &mut ExecutionDeltaEncoder,
        player: &mut ScenePlayer,
    ) -> ExecutionDeltaEnvelope {
        let changes = player.take_frame_changes();
        encoder.encode(player.frame(), &changes).unwrap().unwrap()
    }
""",
    """    fn encode_current(
        encoder: &mut ExecutionDeltaEncoder,
        player: &mut ScenePlayer,
    ) -> ExecutionDeltaEnvelope {
        let changes = player.take_frame_changes();
        let live_indices = player.live_frame_indices();
        encoder
            .encode_live(player.frame(), &changes, &live_indices)
            .unwrap()
            .unwrap()
    }
""",
)

print("applied tombstone-aware transport compatibility")
