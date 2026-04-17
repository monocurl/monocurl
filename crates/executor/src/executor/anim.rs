use std::rc::Rc;

use crate::{
    error::ExecutorError,
    executor::{SeekPrimitiveAnimSkipResult, SeekToResult},
    state::{BakedPrimitiveAnim, ExecutionState},
    time::Timestamp,
    value::{RcValue, Value, anim_block::AnimBlock, leader::Leader, primitive_anim::PrimitiveAnim},
};

use super::{ExecSingle, Executor, SeekPrimitiveResult};

impl Executor {
    /// run all execution heads until each hits a Play instruction or ends.
    /// yields between iterations so the async executor can interrupt if needed.
    async fn seek_primitive_anim(&mut self) -> SeekPrimitiveResult {
        while let Some(&stack_idx) = self.state.execution_heads.first() {
            // run this head until it yields or ends
            let result = loop {
                self.tick_yielder().await;

                let r = self.execute_one(stack_idx).await;
                match r {
                    ExecSingle::Continue => {}
                    other => break other,
                }
            };

            match result {
                // in either of the two cases, this execution head gets removed
                ExecSingle::Play => {}
                ExecSingle::EndOfHead => {}
                ExecSingle::Error(e) => {
                    let error_stack_idx = self.take_error_stack_idx(stack_idx);
                    let runtime_error = self.build_runtime_error(e.clone(), error_stack_idx);
                    self.state.error(runtime_error);
                    return SeekPrimitiveResult::Error(e);
                }
                ExecSingle::Continue => unreachable!(),
            }
        }

        if self.state.primitive_anims.is_empty() {
            return SeekPrimitiveResult::EndOfSection;
        } else {
            return SeekPrimitiveResult::PrimitiveAnim;
        }
    }

    // seek primitive anim, possibly skipping slides
    pub async fn seek_primitive_anim_skip(
        &mut self,
        max_slide: usize,
    ) -> SeekPrimitiveAnimSkipResult {
        loop {
            self.tick_yielder().await;

            match self.seek_primitive_anim().await {
                SeekPrimitiveResult::EndOfSection => {
                    if self.state.timestamp.slide < max_slide
                        && self.state.timestamp.slide + 1 < self.bytecode.sections.len()
                    {
                        self.advance_section().await;
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

    /// step all active primitive animations by dt seconds
    pub async fn step_primitive_anims(&mut self, dt: f64) -> Result<(), ExecutorError> {
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
            if let Err(err) = self.apply_primitive_anim_step(baked, *t).await {
                let runtime_error = self.build_runtime_error(err.clone(), baked.parent_stack_idx);
                self.state.error(runtime_error);
                return Err(err);
            }
        }

        // finalize finished anims (snap to final state), reverse to preserve indices
        for &i in finished_indices.iter().rev() {
            let baked = self.state.primitive_anims.remove(i);
            if let Err(err) = self.apply_primitive_anim_step(&baked, 1.0).await {
                self.release_primitive_anim_locks(&baked);
                let runtime_error = self.build_runtime_error(err.clone(), baked.parent_stack_idx);
                self.state.error(runtime_error);
                return Err(err);
            }
            self.release_primitive_anim_locks(&baked);
            self.resume_parent_after_anim(baked.parent_stack_idx);
        }

        Ok(())
    }

    /// consume playback time continuously, carrying leftover dt across resumed heads
    pub async fn advance_playback(
        &mut self,
        max_slide: usize,
        dt: f64,
    ) -> Result<bool, ExecutorError> {
        debug_assert!(dt >= 0.0);
        self.state.pending_playback_time += dt;

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

            if self.state.pending_playback_time <= f64::EPSILON {
                self.state.pending_playback_time = 0.0;
            }
        }

        Ok(true)
    }

    /// seek to a target timestamp by stepping to the next event (animation end)
    /// rather than fixed dt steps.
    pub async fn seek_to(&mut self, target: Timestamp) -> SeekToResult {
        self.rebase_at_cache_point(target).await;

        loop {
            // find first primitive anim that happens before target
            match self.seek_primitive_anim_skip(target.slide).await {
                SeekPrimitiveAnimSkipResult::PrimitiveAnim => {}
                SeekPrimitiveAnimSkipResult::NoAnimsLeft => {
                    return SeekToResult::SeekedTo(self.state.timestamp);
                }
                SeekPrimitiveAnimSkipResult::Error(e) => return SeekToResult::Error(e),
            }

            if self.state.timestamp.slide == target.slide
                && self.state.timestamp.time >= target.time
            {
                return SeekToResult::SeekedTo(self.state.timestamp);
            }

            // keep going until we finish this group or hit the target
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

            match self.step_primitive_anims(dt).await {
                Ok(_) => {}
                Err(e) => {
                    return SeekToResult::Error(e);
                }
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
            PrimitiveAnim::Lerp { .. } => {
                // TODO: apply progression lambda to remap t
                if t >= 1.0 {
                    for target in &baked.targets {
                        sync_leader_to_follower(target);
                    }
                } else {
                    for (target, start) in baked.targets.iter().zip(&baked.start_followers) {
                        let (leader_value, follower_rc) = {
                            let leader_cell = target.borrow();
                            let Value::Leader(leader) = &*leader_cell else {
                                continue;
                            };
                            (
                                leader.leader_rc.borrow().clone(),
                                leader.follower_rc.clone(),
                            )
                        };
                        let lerped = self.lerp(start.clone(), leader_value, t).await?;
                        *follower_rc.borrow_mut() = lerped;
                    }
                }
            }
        }

        Ok(())
    }

    fn resume_parent_after_anim(&mut self, parent_stack_idx: usize) {
        let parent = self.state.stack_mut(parent_stack_idx);
        parent.active_child_count -= 1;
        if parent.active_child_count == 0 {
            self.state.execution_heads.insert(parent_stack_idx);
        }
    }

    pub(super) fn finish_execution_head(&mut self, stack_idx: usize) {
        let parent_idx = self.state.stack(stack_idx).parent_idx;

        // capture TOS for root stacks so tests can inspect results.
        // peek (clone without pop) so that variables remain on the stack for
        // subsequent sections to access by position — necessary for cross-section
        // imports (e.g. stdlib symbols defined in a library section then used in
        // user slides).
        #[cfg(feature = "capture_tos")]
        if parent_idx.is_none() {
            let stack = self.state.stack(stack_idx);
            if stack.stack_len() > 0 {
                let val = stack.peek().clone().elide_lvalue();
                self.state.captured_output.push(val);
            }
        }

        self.state.execution_heads.remove(&stack_idx);

        // never free the root stack, even on section end
        // this is more reliable check than checking parent index due to dedicated lambda stacks
        if stack_idx != ExecutionState::ROOT_STACK_ID {
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

    pub(super) fn exec_play(&mut self, stack_idx: usize) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let val = stack.pop();

        match val {
            Value::AnimBlock(anim_block) => match self.spawn_anim_block(stack_idx, anim_block) {
                Ok(()) => {
                    self.state.execution_heads.remove(&stack_idx);
                    ExecSingle::Play
                }
                Err(e) => ExecSingle::Error(e),
            },
            Value::PrimitiveAnim(prim) => match self.bake_primitive_anim(stack_idx, prim, &[]) {
                Ok(()) => {
                    self.state.execution_heads.remove(&stack_idx);
                    ExecSingle::Play
                }
                Err(e) => ExecSingle::Error(e),
            },
            Value::List(list) => {
                let values: Vec<Value> = list
                    .elements
                    .iter()
                    .map(|elem| elem.borrow().clone())
                    .collect();
                let mut reserved = Vec::new();
                let mut planned_primitives = Vec::new();
                for elem in &values {
                    if let Value::PrimitiveAnim(pa) = elem {
                        let baked = match self.plan_primitive_anim(stack_idx, pa.clone(), &reserved)
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

    fn bake_primitive_anim(
        &mut self,
        parent_stack_idx: usize,
        prim: PrimitiveAnim,
        reserved: &[RcValue],
    ) -> Result<(), ExecutorError> {
        let baked = self.plan_primitive_anim(parent_stack_idx, prim, reserved)?;
        self.install_baked_primitive_anim(parent_stack_idx, baked);
        Ok(())
    }

    fn plan_primitive_anim(
        &mut self,
        parent_stack_idx: usize,
        prim: PrimitiveAnim,
        reserved: &[RcValue],
    ) -> Result<BakedPrimitiveAnim, ExecutorError> {
        let duration = match &prim {
            PrimitiveAnim::Lerp { time, .. } => *time,
            PrimitiveAnim::Set { .. } => 0.0,
            PrimitiveAnim::Wait { time } => *time,
        };

        let start = self.state.timestamp.time;
        let stack_id = self.state.stack(parent_stack_idx).stack_id;
        let targets = self.resolve_primitive_anim_targets(&prim, reserved)?;
        let start_followers = targets
            .iter()
            .map(|target| {
                let leader_cell = target.borrow();
                let Value::Leader(leader) = &*leader_cell else {
                    unreachable!("planned primitive target must be a leader")
                };
                leader.follower_rc.borrow().clone()
            })
            .collect();

        Ok(BakedPrimitiveAnim {
            anim_id: self.state.alloc_primitive_anim_id(),
            anim: prim,
            start_time: start,
            end_time: start + duration,
            targets,
            start_followers,
            parent_stack_idx,
            stack_id,
            span: self.current_instruction_span(parent_stack_idx),
        })
    }

    fn install_baked_primitive_anim(&mut self, parent_stack_idx: usize, baked: BakedPrimitiveAnim) {
        for target in &baked.targets {
            let mut leader_cell = target.borrow_mut();
            let Value::Leader(leader) = &mut *leader_cell else {
                continue;
            };
            leader.locked_by_anim = Some(baked.anim_id);
        }

        self.state.primitive_anims.push(baked);
        self.state.stack_mut(parent_stack_idx).active_child_count += 1;
    }

    fn resolve_primitive_anim_targets(
        &self,
        prim: &PrimitiveAnim,
        reserved: &[RcValue],
    ) -> Result<Vec<RcValue>, ExecutorError> {
        let mut targets = Vec::new();

        match prim {
            PrimitiveAnim::Lerp { candidates, .. } | PrimitiveAnim::Set { candidates } => {
                self.flatten_candidate_tree(candidates, &mut targets)?;
                if targets.is_empty() {
                    for entry in &self.state.leaders {
                        let leader_cell = entry.leader_cell_rc.borrow();
                        let Value::Leader(leader) = &*leader_cell else {
                            continue;
                        };
                        if leader.last_modified_stack.is_some() {
                            targets.push(entry.leader_cell_rc.clone());
                        }
                    }
                }
            }
            PrimitiveAnim::Wait { .. } => {}
        }

        dedup_rc_values(&mut targets);
        for target in &targets {
            if reserved.iter().any(|reserved| Rc::ptr_eq(reserved, target)) {
                return Err(ExecutorError::ConcurrentAnimation);
            }

            let leader_cell = target.borrow();
            let Value::Leader(leader) = &*leader_cell else {
                return Err(ExecutorError::type_error("leader", leader_cell.type_name()));
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
        out: &mut Vec<RcValue>,
    ) -> Result<(), ExecutorError> {
        match value {
            Value::List(list) => {
                for element in &list.elements {
                    self.flatten_candidate_tree(&element.borrow(), out)?;
                }
                Ok(())
            }
            Value::Lvalue(rc) => self.push_leader_candidate(rc, out),
            Value::WeakLvalue(weak) => {
                let Some(rc) = weak.upgrade() else {
                    return Err(ExecutorError::Other(
                        "animation variable reference expired".into(),
                    ));
                };
                self.push_leader_candidate(&rc, out)
            }
            Value::Leader(leader) => {
                let Some(cell) = self.find_leader_cell(leader) else {
                    return Err(ExecutorError::Other(
                        "animation variable does not belong to executor state".into(),
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
        rc: &RcValue,
        out: &mut Vec<RcValue>,
    ) -> Result<(), ExecutorError> {
        match &*rc.borrow() {
            Value::Leader(_) => {
                out.push(rc.clone());
                Ok(())
            }
            other => Err(ExecutorError::type_error(
                "leader variable reference",
                other.type_name(),
            )),
        }
    }

    fn find_leader_cell(&self, needle: &Leader) -> Option<RcValue> {
        self.state.leaders.iter().find_map(|entry| {
            let leader_cell = entry.leader_cell_rc.borrow();
            let Value::Leader(existing) = &*leader_cell else {
                return None;
            };
            (Rc::ptr_eq(&existing.leader_rc, &needle.leader_rc)
                && Rc::ptr_eq(&existing.follower_rc, &needle.follower_rc))
            .then(|| entry.leader_cell_rc.clone())
        })
    }

    fn release_primitive_anim_locks(&self, baked: &BakedPrimitiveAnim) {
        for target in &baked.targets {
            let mut leader_cell = target.borrow_mut();
            let Value::Leader(leader) = &mut *leader_cell else {
                continue;
            };
            if leader.locked_by_anim == Some(baked.anim_id) {
                leader.locked_by_anim = None;
            }
        }
    }
}

fn dedup_rc_values(values: &mut Vec<RcValue>) {
    let mut deduped = Vec::with_capacity(values.len());
    for value in values.drain(..) {
        if !deduped.iter().any(|existing| Rc::ptr_eq(existing, &value)) {
            deduped.push(value);
        }
    }
    *values = deduped;
}

fn sync_leader_to_follower(leader_cell_rc: &RcValue) {
    let mut leader_cell = leader_cell_rc.borrow_mut();
    let Value::Leader(Leader {
        last_modified_stack,
        leader_rc,
        follower_rc,
        ..
    }) = &mut *leader_cell
    else {
        return;
    };
    let value = leader_rc.borrow().clone();
    *follower_rc.borrow_mut() = value;
    *last_modified_stack = None;
}
