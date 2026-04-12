use structs::futures::{PeriodicYielder};

use crate::{
    error::ExecutorError,
    state::BakedPrimitiveAnim,
    time::Timestamp,
    value::{
        Value,
        anim_block::AnimBlock,
        primitive_anim::PrimitiveAnim,
    },
};

use super::{ExecSingle, Executor, SeekPrimitiveResult, StepResult};

impl Executor {
    /// run all execution heads until each hits a Play instruction or ends.
    /// yields between iterations so the async executor can interrupt if needed.
    pub async fn seek_primitive_anim(&mut self) -> SeekPrimitiveResult {
        let mut yielder = PeriodicYielder::default();

        loop {
            let mut any_progressed = false;
            let mut head_idx = 0;

            while head_idx < self.state.execution_heads.len() {
                let stack_idx = self.state.execution_heads[head_idx];

                // run this head until it yields (Play) or ends
                let result = loop {
                    yielder.tick().await;

                    let r = self.execute_one(stack_idx).await;
                    match r {
                        ExecSingle::Continue => {}
                        other => break other,
                    }
                };

                match result {
                    ExecSingle::Play => {
                        any_progressed = true;
                        head_idx += 1;
                    }
                    ExecSingle::EndOfHead => {
                        self.finish_execution_head(stack_idx);
                        self.state.execution_heads.swap_remove(head_idx);
                        any_progressed = true;
                        // don't increment — swapped element is now at this index
                    }
                    ExecSingle::Error(e) => return SeekPrimitiveResult::Error(e),
                    ExecSingle::Continue => unreachable!(),
                }
            }

            if self.state.execution_heads.is_empty() && self.state.primitive_anims.is_empty() {
                return SeekPrimitiveResult::EndOfSection;
            }

            if !self.state.primitive_anims.is_empty() {
                return SeekPrimitiveResult::PrimitiveAnim;
            }

            // if no heads progressed and no anims baked, we're stuck
            if !any_progressed {
                return SeekPrimitiveResult::EndOfSection;
            }
        }
    }

    /// step all active primitive animations by dt seconds.
    pub async fn step_primitive_anims(&mut self, dt: f64) -> StepResult {
        self.current_play_time += dt;
        self.state.timestamp.time += dt;

        let mut finished_indices = Vec::new();
        let mut in_progress = Vec::new();
        for (i, baked) in self.state.primitive_anims.iter().enumerate() {
            if self.current_play_time >= baked.end_time {
                finished_indices.push(i);
            } else {
                let t = if baked.end_time > baked.start_time {
                    (self.current_play_time - baked.start_time)
                        / (baked.end_time - baked.start_time)
                } else {
                    1.0
                };
                in_progress.push((baked.anim.clone(), t));
            }
        }

        for (anim, t) in &in_progress {
            self.apply_primitive_anim_step(anim, *t);
        }

        // finalize finished anims (snap to final state), reverse to preserve indices
        for &i in finished_indices.iter().rev() {
            let baked = self.state.primitive_anims.remove(i);
            self.apply_primitive_anim_step(&baked.anim, 1.0);
            self.resume_parent_after_anim(baked.parent_stack_idx);
        }

        if self.state.primitive_anims.is_empty() {
            StepResult::EndOfAllAnims
        } else {
            StepResult::Continue
        }
    }

    /// seek to a target timestamp by stepping to the next event (animation end)
    /// rather than fixed dt steps.
    pub async fn seek_to(&mut self, target: Timestamp) {
        while self.state.timestamp.slide <= target.slide {
            match self.seek_primitive_anim().await {
                SeekPrimitiveResult::EndOfSection => break,
                SeekPrimitiveResult::Error(e) => {
                    self.state.error(e.to_string());
                    break;
                }
                SeekPrimitiveResult::PrimitiveAnim => {}
            }

            loop {
                if self.state.timestamp.time >= target.time
                    && self.state.timestamp.slide >= target.slide
                {
                    return;
                }

                let next_end = self
                    .state
                    .primitive_anims
                    .iter()
                    .map(|b| b.end_time)
                    .fold(f64::INFINITY, f64::min);

                let step_target = next_end.min(target.time);
                let dt = (step_target - self.current_play_time).max(0.0);

                if dt <= 0.0 && self.state.primitive_anims.is_empty() {
                    break;
                }

                match self.step_primitive_anims(dt.max(f64::MIN_POSITIVE)).await {
                    StepResult::EndOfAllAnims => break,
                    StepResult::Continue => {}
                    StepResult::Error(e) => {
                        self.state.error(e.to_string());
                        return;
                    }
                }
            }
        }
    }

    fn apply_primitive_anim_step(&mut self, anim: &PrimitiveAnim, t: f64) {
        match anim {
            PrimitiveAnim::Set => {
                for entry in &self.state.leaders {
                    let leader_val = entry.leader_rc.borrow();
                    let Value::Leader(leader) = &*leader_val else { continue };
                    let val = leader.leader_rc.borrow().clone();
                    *leader.follower_rc.borrow_mut() = val;
                }
            }
            PrimitiveAnim::Wait { .. } => {}
            PrimitiveAnim::Lerp { .. } => {
                // TODO: apply progression lambda to remap t
                // TODO: actual interpolation (generalized lerp)
                if t >= 1.0 {
                    for entry in &self.state.leaders {
                        let leader_val = entry.leader_rc.borrow();
                        let Value::Leader(leader) = &*leader_val else { continue };
                        let val = leader.leader_rc.borrow().clone();
                        *leader.follower_rc.borrow_mut() = val;
                    }
                }
            }
        }
    }

    fn resume_parent_after_anim(&mut self, parent_stack_idx: usize) {
        let parent = self.state.stack_mut(parent_stack_idx);
        parent.active_child_count -= 1;
        if parent.active_child_count == 0 {
            self.state.execution_heads.push(parent_stack_idx);
        }
    }

    pub(super) fn finish_execution_head(&mut self, stack_idx: usize) {
        let parent_idx = self.state.stack(stack_idx).parent_idx;

        // capture TOS for root stacks (no parent) so tests can inspect results
        #[cfg(feature = "capture_tos")]
        if parent_idx.is_none() {
            let stack = self.state.stack_mut(stack_idx);
            if stack.stack_len() > 0 {
                let val = stack.pop().elide_lvalue();
                self.state.captured_output.push(val);
            }
        }

        self.state.free_stack(stack_idx);
        if let Some(parent) = parent_idx {
            let p = self.state.stack_mut(parent);
            p.active_child_count -= 1;
            if p.active_child_count == 0 {
                self.state.execution_heads.push(parent);
            }
        }
    }

    pub(super) fn exec_play(&mut self, stack_idx: usize) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let val = stack.pop();

        match val {
            Value::AnimBlock(anim_block) => {
                match self.spawn_anim_block(stack_idx, anim_block) {
                    Ok(()) => ExecSingle::Play,
                    Err(e) => ExecSingle::Error(e),
                }
            }
            Value::PrimitiveAnim(prim) => {
                self.bake_primitive_anim(stack_idx, prim);
                ExecSingle::Play
            }
            Value::List(list) => {
                let mut count = 0;
                for elem_rc in &list.elements {
                    let elem = elem_rc.borrow().clone();
                    match elem {
                        Value::AnimBlock(ab) => {
                            match self.spawn_anim_block(stack_idx, ab) {
                                Ok(()) => count += 1,
                                Err(e) => return ExecSingle::Error(e),
                            }
                        }
                        Value::PrimitiveAnim(pa) => {
                            self.bake_primitive_anim(stack_idx, pa);
                            count += 1;
                        }
                        _ => {
                            return ExecSingle::Error(ExecutorError::type_error(
                                "anim_block or primitive_anim",
                                elem.type_name(),
                            ))
                        }
                    }
                }
                if count == 0 {
                    ExecSingle::Continue
                } else {
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

        let child_idx = self.state.alloc_stack(anim_block.ip, Some(parent_stack_idx)).map_err(|_| ExecutorError::TooManyActiveAnimations)?;
        let child = self.state.stack_mut(child_idx);
        for cap in &anim_block.captures {
            child.push(cap.clone());
        }
        self.state.stack_mut(parent_stack_idx).active_child_count += 1;
        self.state.execution_heads.push(child_idx);
        Ok(())
    }

    fn bake_primitive_anim(&mut self, parent_stack_idx: usize, prim: PrimitiveAnim) {
        let duration = match &prim {
            PrimitiveAnim::Lerp { time, .. } => *time,
            PrimitiveAnim::Set => 0.0,
            PrimitiveAnim::Wait { time } => *time,
        };

        let start = self.current_play_time;
        let stack_id = self.state.stack(parent_stack_idx).stack_id;

        self.state.primitive_anims.push(BakedPrimitiveAnim {
            anim: prim,
            start_time: start,
            end_time: start + duration,
            parent_stack_idx,
            stack_id,
        });
        self.state.stack_mut(parent_stack_idx).active_child_count += 1;
    }
}
