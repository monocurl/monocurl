use std::collections::BTreeSet;

use crate::{
    error::ExecutorError,
    executor::{SeekPrimitiveAnimSkipResult, SeekToResult},
    heap::{VRc, heap_replace, with_heap, with_heap_mut},
    state::{BakedPrimitiveAnim, ExecutionState},
    time::Timestamp,
    value::{Value, anim_block::AnimBlock, leader::Leader, primitive_anim::PrimitiveAnim},
};

use super::{ExecSingle, Executor, SeekPrimitiveResult, prepare_eager_call_args};

impl Executor {
    fn primitive_anim_duration(prim: &PrimitiveAnim) -> f64 {
        match prim {
            PrimitiveAnim::Lerp { time, .. } => *time,
            PrimitiveAnim::Set { .. } => 0.0,
            PrimitiveAnim::Wait { time } => *time,
        }
    }

    pub fn advance_section(&mut self) {
        debug_assert!(self.state.execution_heads.is_empty());

        self.save_cache();

        let mut heads = BTreeSet::new();
        heads.insert(ExecutionState::ROOT_STACK_IDX);

        self.state.execution_heads = heads;

        let ip = ((self.state.timestamp.slide + 1) as u16, 0);
        self.state.stack_mut(ExecutionState::ROOT_STACK_IDX).ip = ip;
        self.state.timestamp.slide += 1;
        // "unplayed" state
        self.state.timestamp.time = -f64::MIN_POSITIVE;
    }

    async fn seek_primitive_anim(&mut self) -> SeekPrimitiveResult {
        // mark no longer unplayed

        while let Some(&stack_idx) = self.state.execution_heads.first() {
            let result = loop {
                self.tick_yielder().await;

                let r = self.execute_one(stack_idx).await;
                match r {
                    ExecSingle::Continue => {}
                    other => break other,
                }
            };

            match result {
                ExecSingle::Play => {}
                ExecSingle::EndOfHead => {}
                ExecSingle::Error(e) => {
                    let runtime_error = self.build_runtime_error(e.clone());
                    self.state.error(runtime_error);
                    return SeekPrimitiveResult::Error(e);
                }
                ExecSingle::Continue => unreachable!(),
            }
        }

        if self.state.primitive_anims.is_empty() {
            SeekPrimitiveResult::EndOfSection
        } else {
            SeekPrimitiveResult::PrimitiveAnim
        }
    }

    async fn seek_primitive_anim_skip(&mut self, max_slide: usize) -> SeekPrimitiveAnimSkipResult {
        loop {
            self.tick_yielder().await;

            match self.seek_primitive_anim().await {
                SeekPrimitiveResult::EndOfSection => {
                    if self.state.timestamp.slide < max_slide
                        && self.state.timestamp.slide + 1 < self.bytecode.sections.len()
                    {
                        self.advance_section();
                    } else {
                        self.save_cache();
                        return SeekPrimitiveAnimSkipResult::NoAnimsLeft;
                    }
                }
                SeekPrimitiveResult::Error(e) => {
                    return SeekPrimitiveAnimSkipResult::Error(e);
                }
                SeekPrimitiveResult::PrimitiveAnim => break,
            }
        }

        SeekPrimitiveAnimSkipResult::PrimitiveAnim
    }

    async fn step_primitive_anims(&mut self, dt: f64) -> Result<(), ExecutorError> {
        debug_assert!(self.state.execution_heads.is_empty());
        self.state.timestamp.time += dt;
        self.note_current_timestamp_in_cache();

        let mut finished_indices = Vec::new();
        let mut in_progress = Vec::new();
        for (i, baked) in self.state.primitive_anims.iter().enumerate() {
            if self.state.timestamp.time >= baked.end_time {
                finished_indices.push(i);
            } else {
                let t = if baked.end_time > baked.start_time {
                    (self.state.timestamp.time - baked.start_time)
                        / (baked.end_time - baked.start_time)
                } else {
                    1.0
                };
                in_progress.push((baked.clone(), t));
            }
        }

        for (baked, t) in &in_progress {
            self.state.last_stack_idx = baked.parent_stack_idx;
            if let Err(err) = self.apply_primitive_anim_step(baked, *t).await {
                let runtime_error = self.build_runtime_error(err.clone());
                self.state.error(runtime_error);
                return Err(err);
            }
        }

        for &i in finished_indices.iter().rev() {
            let baked = self.state.primitive_anims.remove(i);
            self.state.last_stack_idx = baked.parent_stack_idx;
            if let Err(err) = self.apply_primitive_anim_step(&baked, 1.0).await {
                self.release_primitive_anim_locks(&baked);
                let runtime_error = self.build_runtime_error(err.clone());
                self.state.error(runtime_error);
                return Err(err);
            }
            self.release_primitive_anim_locks(&baked);
            self.resume_parent_after_anim(baked.parent_stack_idx);
        }

        Ok(())
    }

    /// returns Ok(true) if there may be more anims to step through, Ok(false) if we've reached the end of the section, or Err if an error was encountered
    pub async fn advance_playback(
        &mut self,
        max_slide: usize,
        dt: f64,
    ) -> Result<bool, ExecutorError> {
        debug_assert!(dt >= 0.0);
        self.state.pending_playback_time += dt;
        self.state.timestamp.time = self.state.timestamp.time.max(0.0);

        while self.state.pending_playback_time > 0.0 {
            match self.seek_primitive_anim_skip(max_slide).await {
                SeekPrimitiveAnimSkipResult::PrimitiveAnim => {}
                SeekPrimitiveAnimSkipResult::NoAnimsLeft => {
                    self.state.pending_playback_time = 0.0;
                    return Ok(false);
                }
                SeekPrimitiveAnimSkipResult::Error(e) => {
                    self.state.pending_playback_time = 0.0;
                    return Err(e);
                }
            }

            let next_end = self
                .state
                .primitive_anims
                .iter()
                .map(|b| b.end_time)
                .fold(f64::INFINITY, f64::min);

            let step_dt = (next_end - self.state.timestamp.time)
                .min(self.state.pending_playback_time)
                .max(0.0);

            self.step_primitive_anims(step_dt).await?;
            self.state.pending_playback_time -= step_dt;
        }

        Ok(true)
    }

    pub async fn seek_to(&mut self, target: Timestamp) -> SeekToResult {
        self.rebase_at_cache_point(target);

        loop {
            match self.seek_primitive_anim_skip(target.slide).await {
                SeekPrimitiveAnimSkipResult::PrimitiveAnim => {}
                SeekPrimitiveAnimSkipResult::NoAnimsLeft => {
                    // would the target allowed us to have played anything at all
                    if self.state.timestamp.slide == target.slide && target.time >= 0.0 {
                        self.state.timestamp.time = self.state.timestamp.time.max(0.0);
                    }
                    return SeekToResult::SeekedTo(self.state.timestamp);
                }
                SeekPrimitiveAnimSkipResult::Error(e) => return SeekToResult::Error(e),
            }

            if self.state.timestamp.slide == target.slide
                && self.state.timestamp.time >= target.time
            {
                return SeekToResult::SeekedTo(self.state.timestamp);
            }
            self.state.timestamp.time = self.state.timestamp.time.max(0.0);

            let next_end = self
                .state
                .primitive_anims
                .iter()
                .map(|b| b.end_time)
                .fold(f64::INFINITY, f64::min);

            let step_target = next_end.min(if self.state.timestamp.slide < target.slide {
                f64::INFINITY
            } else {
                target.time
            });
            let dt = step_target - self.state.timestamp.time;

            if let Err(e) = self.step_primitive_anims(dt).await {
                return SeekToResult::Error(e);
            }
        }
    }

    async fn apply_primitive_anim_step(
        &mut self,
        baked: &BakedPrimitiveAnim,
        t: f64,
    ) -> Result<(), ExecutorError> {
        match &baked.anim {
            PrimitiveAnim::Set { .. } => {
                for target in &baked.targets {
                    sync_leader_to_follower(target);
                }
            }
            PrimitiveAnim::Wait { .. } => {}
            PrimitiveAnim::Lerp { lerp, .. } => {
                if t >= 1.0 {
                    for (target, destination) in baked.targets.iter().zip(&baked.destinations) {
                        sync_baked_destination_to_follower(target, destination.clone());
                    }
                } else {
                    let t = self.eval_lerp_t(baked, t).await?;
                    for (((target, start), end), state) in baked
                        .targets
                        .iter()
                        .zip(&baked.embedded_starts)
                        .zip(&baked.embedded_ends)
                        .zip(&baked.embedded_states)
                    {
                        let lerped = if let Some(lerp) = lerp {
                            self.eval_custom_lerp_value(
                                lerp,
                                baked.parent_stack_idx,
                                start.clone(),
                                end.clone(),
                                state.clone(),
                                t,
                            )
                            .await?
                        } else {
                            self.lerp(start.clone(), end.clone(), t).await?
                        };
                        replace_follower_value(target, lerped);
                    }
                }
            }
        }

        Ok(())
    }

    async fn eval_lerp_t(
        &mut self,
        baked: &BakedPrimitiveAnim,
        linear_t: f64,
    ) -> Result<f64, ExecutorError> {
        let PrimitiveAnim::Lerp { progression, .. } = &baked.anim else {
            return Ok(linear_t);
        };
        let Some(progression) = progression else {
            return Ok(linear_t);
        };

        let raw = self
            .eval_anim_callable(
                progression,
                baked.parent_stack_idx,
                vec![Value::Float(linear_t)],
                "rate",
            )
            .await?;

        match raw.elide_lvalue() {
            Value::Float(t) => Ok(t),
            Value::Integer(t) => Ok(t as f64),
            other => Err(ExecutorError::type_error_for(
                "float",
                other.type_name(),
                "rate",
            )),
        }
    }

    async fn eval_embed_value(
        &mut self,
        embed: &Value,
        parent_stack_idx: usize,
        start: Value,
        destination: Value,
    ) -> Result<(Value, Value, Value), ExecutorError> {
        let raw = self
            .eval_anim_callable(embed, parent_stack_idx, vec![start, destination], "embed")
            .await?;
        unpack_embed_triplet(raw)
    }

    async fn eval_custom_lerp_value(
        &mut self,
        lerp: &Value,
        parent_stack_idx: usize,
        start: Value,
        end: Value,
        state: Value,
        t: f64,
    ) -> Result<Value, ExecutorError> {
        self.eval_anim_callable(
            lerp,
            parent_stack_idx,
            vec![start, end, state, Value::Float(t)],
            "lerp",
        )
        .await
    }

    async fn eval_anim_callable(
        &mut self,
        callable: &Value,
        parent_stack_idx: usize,
        args: Vec<Value>,
        target: &'static str,
    ) -> Result<Value, ExecutorError> {
        let mut materialized_args = Vec::with_capacity(args.len());
        for arg in args {
            materialized_args.push(self.materialize_cached_value(arg).await?);
        }

        match callable.clone().elide_lvalue() {
            Value::Lambda(lambda) => {
                let args = prepare_eager_call_args(materialized_args, &lambda)?;
                self.eagerly_invoke_lambda(&lambda, &args, Some(parent_stack_idx))
                    .await
            }
            Value::Operator(operator) => {
                let args = prepare_eager_call_args(materialized_args, &operator.0)?;
                self.eagerly_invoke_lambda(&operator.0, &args, Some(parent_stack_idx))
                    .await
            }
            other => Err(ExecutorError::type_error_for(
                "lambda / operator",
                other.type_name(),
                target,
            )),
        }
    }

    fn resume_parent_after_anim(&mut self, parent_stack_idx: usize) {
        let parent = self.state.stack_mut(parent_stack_idx);
        parent.active_child_count -= 1;
        if parent.active_child_count == 0 {
            self.state.execution_heads.insert(parent_stack_idx);
        }
    }

    pub(super) fn finish_execution_head(&mut self, stack_idx: usize) {
        let parent_idx = self.state.stack_parent_idx(stack_idx);

        #[cfg(feature = "capture_tos")]
        if parent_idx.is_none() {
            let stack = self.state.stack(stack_idx);
            if stack.stack_len() > 0 {
                let val = stack.peek().clone().elide_lvalue();
                self.state.captured_output.push(val);
            }
        }

        self.state.execution_heads.remove(&stack_idx);

        if stack_idx != ExecutionState::ROOT_STACK_IDX {
            self.state.free_stack(stack_idx);
        }

        if let Some(parent) = parent_idx {
            let p = self.state.stack_mut(parent);
            p.active_child_count -= 1;
            if p.active_child_count == 0 {
                self.state.execution_heads.insert(parent);
            }
        }
    }

    pub(super) async fn exec_play(&mut self, stack_idx: usize) -> ExecSingle {
        let val = self.state.stack_mut(stack_idx).pop();
        let val = match val.elide_wrappers_rec(self).await {
            Ok(val) => val,
            Err(e) => return ExecSingle::Error(e),
        };

        match val {
            Value::AnimBlock(anim_block) => match self.spawn_anim_block(stack_idx, anim_block) {
                Ok(()) => {
                    self.state.execution_heads.remove(&stack_idx);
                    ExecSingle::Play
                }
                Err(e) => ExecSingle::Error(e),
            },
            Value::PrimitiveAnim(prim) => {
                match self.bake_primitive_anim(stack_idx, prim, &[]).await {
                    Ok(()) => {
                        self.state.execution_heads.remove(&stack_idx);
                        ExecSingle::Play
                    }
                    Err(e) => ExecSingle::Error(e),
                }
            }
            Value::List(list) => {
                let mut values = Vec::with_capacity(list.elements.len());
                for key in list.elements.iter() {
                    let elem = with_heap(|h| h.get(key.key()).clone());
                    let elem = match elem.elide_wrappers_rec(self).await {
                        Ok(elem) => elem,
                        Err(e) => return ExecSingle::Error(e),
                    };
                    values.push(elem);
                }
                let mut reserved = Vec::new();
                let mut planned_primitives = Vec::new();
                for elem in &values {
                    if let Value::PrimitiveAnim(pa) = elem {
                        let baked = match self
                            .plan_primitive_anim(stack_idx, pa.clone(), &reserved)
                            .await
                        {
                            Ok(baked) => baked,
                            Err(e) => return ExecSingle::Error(e),
                        };
                        reserved.extend(baked.targets.iter().cloned());
                        planned_primitives.push(baked);
                    }
                }

                let mut count = 0;
                for elem in values {
                    match elem {
                        Value::AnimBlock(ab) => match self.spawn_anim_block(stack_idx, ab) {
                            Ok(()) => count += 1,
                            Err(e) => return ExecSingle::Error(e),
                        },
                        Value::PrimitiveAnim(_) => {}
                        _ => {
                            return ExecSingle::Error(ExecutorError::type_error(
                                "anim_block or primitive_anim",
                                elem.type_name(),
                            ));
                        }
                    }
                }
                for baked in planned_primitives {
                    self.install_baked_primitive_anim(stack_idx, baked);
                    count += 1;
                }
                if count == 0 {
                    ExecSingle::Continue
                } else {
                    self.state.execution_heads.remove(&stack_idx);
                    ExecSingle::Play
                }
            }
            _ => ExecSingle::Error(ExecutorError::type_error(
                "anim_block, primitive_anim, or list",
                val.type_name(),
            )),
        }
    }

    fn spawn_anim_block(
        &mut self,
        parent_stack_idx: usize,
        anim_block: std::rc::Rc<AnimBlock>,
    ) -> Result<(), ExecutorError> {
        if anim_block.already_played.get() {
            return Err(ExecutorError::AnimPlayedTwice);
        }
        anim_block.already_played.set(true);

        let child_idx = self
            .state
            .alloc_stack(
                anim_block.ip,
                Some(parent_stack_idx),
                Some(parent_stack_idx),
            )
            .map_err(|_| ExecutorError::TooManyActiveAnimations)?;
        let child = self.state.stack_mut(child_idx);
        for cap in &anim_block.captures {
            child.push(cap.clone());
        }
        self.state.stack_mut(parent_stack_idx).active_child_count += 1;
        self.state.execution_heads.insert(child_idx);
        Ok(())
    }

    async fn bake_primitive_anim(
        &mut self,
        parent_stack_idx: usize,
        prim: PrimitiveAnim,
        reserved: &[VRc],
    ) -> Result<(), ExecutorError> {
        let baked = self
            .plan_primitive_anim(parent_stack_idx, prim, reserved)
            .await?;
        self.install_baked_primitive_anim(parent_stack_idx, baked);
        Ok(())
    }

    async fn plan_primitive_anim(
        &mut self,
        parent_stack_idx: usize,
        prim: PrimitiveAnim,
        reserved: &[VRc],
    ) -> Result<BakedPrimitiveAnim, ExecutorError> {
        let duration = Self::primitive_anim_duration(&prim);

        let start = self.state.timestamp.time;
        let targets = self.resolve_primitive_anim_targets(parent_stack_idx, &prim, reserved)?;
        let embed = match &prim {
            PrimitiveAnim::Lerp { embed, .. } => embed.as_deref().cloned(),
            _ => None,
        };

        let mut destinations = Vec::with_capacity(targets.len());
        let mut embedded_starts = Vec::with_capacity(targets.len());
        let mut embedded_ends = Vec::with_capacity(targets.len());
        let mut embedded_states = Vec::with_capacity(targets.len());

        for target in &targets {
            let cell_val = with_heap(|h| h.get(target.key()).clone());
            let Value::Leader(leader) = cell_val else {
                unreachable!("planned primitive target must be a leader");
            };

            let follower = with_heap(|h| h.get(leader.follower_rc.key()).clone());
            let destination =
                with_heap(|h| h.get(leader.leader_rc.key()).clone()).to_follower_stateful();
            let (embedded_start, embedded_end, embedded_state) = if let Some(embed) = &embed {
                self.eval_embed_value(embed, parent_stack_idx, follower, destination.clone())
                    .await?
            } else {
                (follower, destination.clone(), Value::Nil)
            };

            destinations.push(destination);
            embedded_starts.push(embedded_start);
            embedded_ends.push(embedded_end);
            embedded_states.push(embedded_state);
        }

        Ok(BakedPrimitiveAnim {
            anim_id: self.state.alloc_primitive_anim_id(),
            anim: prim,
            start_time: start,
            end_time: start + duration,
            targets,
            destinations,
            embedded_starts,
            embedded_ends,
            embedded_states,
            parent_stack_idx,
            span: self.current_instruction_span(parent_stack_idx),
        })
    }

    fn install_baked_primitive_anim(&mut self, parent_stack_idx: usize, baked: BakedPrimitiveAnim) {
        for target in &baked.targets {
            with_heap_mut(|h| {
                if let Value::Leader(leader) = &mut *h.get_mut(target.key()) {
                    leader.locked_by_anim = Some(baked.anim_id);
                }
            });
        }

        self.state.primitive_anims.push(baked);
        self.state.stack_mut(parent_stack_idx).active_child_count += 1;
    }

    fn resolve_primitive_anim_targets(
        &self,
        spawning_stack_idx: usize,
        prim: &PrimitiveAnim,
        reserved: &[VRc],
    ) -> Result<Vec<VRc>, ExecutorError> {
        let mut targets = Vec::new();
        let mut implicit_targets = false;

        match prim {
            PrimitiveAnim::Lerp { candidates, .. } | PrimitiveAnim::Set { candidates } => {
                self.flatten_candidate_tree(candidates, &mut targets)?;
                if targets.is_empty() {
                    implicit_targets = true;
                    for entry in &self.state.leaders {
                        let cell_val = with_heap(|h| h.get(entry.leader_cell.key()).clone());
                        let Value::Leader(leader) = cell_val else {
                            continue;
                        };
                        if leader
                            .last_modified_stack
                            .is_some_and(|last_modified_stack_idx| {
                                self.state.is_stack_ancestor_of_stack(
                                    last_modified_stack_idx,
                                    spawning_stack_idx,
                                )
                            })
                        {
                            targets.push(entry.leader_cell.clone());
                        }
                    }
                }
            }
            PrimitiveAnim::Wait { .. } => {}
        }

        dedup_vrc_targets(&mut targets);
        if implicit_targets {
            targets.retain(|target| {
                if reserved.iter().any(|r| r.key() == target.key()) {
                    return false;
                }
                let cell_val = with_heap(|h| h.get(target.key()).clone());
                let Value::Leader(leader) = cell_val else {
                    return false;
                };
                leader.locked_by_anim.is_none()
            });
        }

        for target in &targets {
            if reserved.iter().any(|r| r.key() == target.key()) {
                return Err(ExecutorError::ConcurrentAnimation);
            }

            let cell_val = with_heap(|h| h.get(target.key()).clone());
            let Value::Leader(leader) = cell_val else {
                return Err(ExecutorError::type_error("leader", cell_val.type_name()));
            };
            if leader.locked_by_anim.is_some() {
                return Err(ExecutorError::ConcurrentAnimation);
            }
        }

        Ok(targets)
    }

    fn flatten_candidate_tree(
        &self,
        value: &Value,
        out: &mut Vec<VRc>,
    ) -> Result<(), ExecutorError> {
        match value {
            Value::List(list) => {
                for elem_key in &list.elements {
                    let elem_val = with_heap(|h| h.get(elem_key.key()).clone());
                    self.flatten_candidate_tree(&elem_val, out)?;
                }
                Ok(())
            }
            Value::Lvalue(vrc) => self.push_leader_candidate(vrc.key(), out),
            Value::WeakLvalue(vweak) => self.push_leader_candidate(vweak.key(), out),
            Value::Leader(leader) => {
                let Some(cell) = self.find_leader_cell(leader) else {
                    return Err(ExecutorError::internal(
                        "animation variable does not belong to executor state",
                    ));
                };
                out.push(cell);
                Ok(())
            }
            other => Err(ExecutorError::type_error(
                "leader variable reference or list",
                other.type_name(),
            )),
        }
    }

    fn push_leader_candidate(
        &self,
        key: crate::heap::HeapKey,
        out: &mut Vec<VRc>,
    ) -> Result<(), ExecutorError> {
        let val = with_heap(|h| h.get(key).clone());
        match val {
            Value::Leader(_) => {
                out.push(VRc::retain_key(key));
                Ok(())
            }
            other => Err(ExecutorError::type_error(
                "leader variable reference",
                other.type_name(),
            )),
        }
    }

    fn find_leader_cell(&self, needle: &Leader) -> Option<VRc> {
        self.state.leaders.iter().find_map(|entry| {
            let cell_val = with_heap(|h| h.get(entry.leader_cell.key()).clone());
            let Value::Leader(existing) = cell_val else {
                return None;
            };
            (existing.leader_rc == needle.leader_rc && existing.follower_rc == needle.follower_rc)
                .then(|| entry.leader_cell.clone())
        })
    }

    fn release_primitive_anim_locks(&self, baked: &BakedPrimitiveAnim) {
        for target in &baked.targets {
            with_heap_mut(|h| {
                if let Value::Leader(leader) = &mut *h.get_mut(target.key()) {
                    if leader.locked_by_anim == Some(baked.anim_id) {
                        leader.locked_by_anim = None;
                    }
                }
            });
        }
    }
}

fn unpack_embed_triplet(raw: Value) -> Result<(Value, Value, Value), ExecutorError> {
    let list = match raw.elide_lvalue() {
        Value::List(list) => list,
        other => {
            return Err(ExecutorError::type_error_for(
                "[a, b, state]",
                other.type_name(),
                "embed",
            ));
        }
    };
    if list.len() != 3 {
        return Err(ExecutorError::InvalidArgument {
            arg: "embed",
            message: "must return [a, b, state]",
        });
    }

    Ok((
        with_heap(|h| h.get(list.elements()[0].key()).clone()),
        with_heap(|h| h.get(list.elements()[1].key()).clone()),
        with_heap(|h| h.get(list.elements()[2].key()).clone()),
    ))
}

fn dedup_vrc_targets(values: &mut Vec<VRc>) {
    let mut deduped = Vec::with_capacity(values.len());
    for value in values.drain(..) {
        if !deduped
            .iter()
            .any(|existing: &VRc| existing.key() == value.key())
        {
            deduped.push(value);
        }
    }
    *values = deduped;
}

fn sync_leader_to_follower(leader_cell: &VRc) {
    let cell_key = leader_cell.key();
    let cell_val = with_heap(|h| h.get(cell_key).clone());
    let Value::Leader(leader) = cell_val else {
        return;
    };
    let value = with_heap(|h| h.get(leader.leader_rc.key()).clone()).to_follower_stateful();
    heap_replace(leader.follower_rc.key(), value);
    with_heap_mut(|h| {
        if let Value::Leader(l) = &mut *h.get_mut(cell_key) {
            l.last_modified_stack = None;
            l.follower_version += 1;
        }
    });
}

fn sync_baked_destination_to_follower(leader_cell: &VRc, value: Value) {
    let cell_key = leader_cell.key();
    let cell_val = with_heap(|h| h.get(cell_key).clone());
    let Value::Leader(leader) = cell_val else {
        return;
    };
    heap_replace(leader.follower_rc.key(), value);
    with_heap_mut(|h| {
        if let Value::Leader(l) = &mut *h.get_mut(cell_key) {
            l.last_modified_stack = None;
            l.follower_version += 1;
        }
    });
}

fn replace_follower_value(leader_cell: &VRc, value: Value) {
    let cell_key = leader_cell.key();
    let cell_val = with_heap(|h| h.get(cell_key).clone());
    let Value::Leader(leader) = cell_val else {
        return;
    };
    heap_replace(leader.follower_rc.key(), value);
    with_heap_mut(|h| {
        if let Value::Leader(l) = &mut *h.get_mut(cell_key) {
            l.follower_version += 1;
        }
    });
}
