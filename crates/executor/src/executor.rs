pub mod access;
pub mod anim;
pub mod cacheing;
pub mod invoke;
pub mod lerp;
pub mod ops;

use std::collections::BTreeSet;
use std::pin::Pin;
use std::{future::Future, rc::Rc};

use bytecode::{Bytecode, Instruction};
use structs::futures::PeriodicYielder;
use structs::text::Span8;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use crate::executor::cacheing::ExecutionCache;
use crate::time::Timestamp;
use crate::{
    error::ExecutorError,
    state::{ExecutionState, LeaderKind},
    value::{
        InstructionPointer, RcValue, Value,
        container::{List, Map},
        lambda::Operator,
        stateful::{Stateful, StatefulNode},
    },
};

pub type StdlibReturn<'a> = Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>>;

pub type StdlibFunc = for<'a> fn(&'a mut Executor, usize) -> StdlibReturn<'a>;

enum SeekPrimitiveResult {
    Error(ExecutorError),
    EndOfSection,
    PrimitiveAnim,
}

pub enum SeekPrimitiveAnimSkipResult {
    Error(ExecutorError),
    PrimitiveAnim,
    NoAnimsLeft,
}

pub enum SeekToResult {
    Error(ExecutorError),
    SeekedTo(Timestamp),
}

/// result of executing a single instruction
pub(crate) enum ExecSingle {
    Continue,
    Play,
    EndOfHead,
    Error(ExecutorError),
}

const EXECUTOR_MEMORY_LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MEMORY_CHECK_PERIOD: u32 = 8_192;

struct PeriodicMemoryChecker {
    count: u32,
    period: u32,
    pid: Option<Pid>,
    system: System,
    limit_bytes: u64,
}

impl PeriodicMemoryChecker {
    fn new(limit_bytes: u64, period: u32) -> Self {
        let pid = sysinfo::get_current_pid().ok();
        let refresh_kind =
            RefreshKind::new().with_processes(ProcessRefreshKind::new().with_memory());

        Self {
            count: 0,
            period,
            pid,
            system: System::new_with_specifics(refresh_kind),
            limit_bytes,
        }
    }

    fn tick(&mut self) -> Result<(), ExecutorError> {
        self.count += 1;
        if self.count < self.period {
            return Ok(());
        }
        self.count = 0;

        let Some(pid) = self.pid else {
            return Ok(());
        };

        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            ProcessRefreshKind::new().with_memory(),
        );

        let Some(process) = self.system.process(pid) else {
            return Ok(());
        };

        let used = process.memory();
        if used > self.limit_bytes {
            return Err(ExecutorError::MemoryLimitExceeded {
                used,
                limit: self.limit_bytes,
            });
        }

        Ok(())
    }
}

pub struct Executor {
    pub state: ExecutionState,
    pub(crate) bytecode: Bytecode,
    pub(crate) native_funcs: Vec<StdlibFunc>,
    pub(crate) cache: ExecutionCache,
    pub(crate) yielder: PeriodicYielder,
    memory_checker: PeriodicMemoryChecker,
    pending_error_stack_idx: Option<usize>,
}

impl Executor {
    pub fn new(bytecode: Bytecode, native_funcs: Vec<StdlibFunc>) -> Self {
        let cache = ExecutionCache::new(&bytecode);
        Self {
            state: ExecutionState::new(),
            bytecode,
            native_funcs,
            cache,
            yielder: PeriodicYielder::default(),
            memory_checker: PeriodicMemoryChecker::new(
                EXECUTOR_MEMORY_LIMIT_BYTES,
                MEMORY_CHECK_PERIOD,
            ),
            pending_error_stack_idx: None,
        }
    }

    pub(crate) async fn tick_yielder(&mut self) {
        self.yielder.tick().await;
    }

    pub fn total_sections(&self) -> usize {
        self.bytecode.sections.len()
    }

    pub fn user_to_internal_timestamp(&self, user_ts: Timestamp) -> Timestamp {
        Timestamp {
            slide: user_ts.slide + self.bytecode.non_slide_sections(),
            time: user_ts.time,
        }
    }

    pub fn internal_to_user_timestamp(&self, internal_ts: Timestamp) -> Timestamp {
        Timestamp {
            slide: internal_ts
                .slide
                .saturating_sub(self.bytecode.non_slide_sections()),
            time: internal_ts.time,
        }
    }

    pub async fn advance_section(&mut self) {
        debug_assert!(self.state.execution_heads.is_empty());

        self.save_cache();

        let mut heads = BTreeSet::new();
        heads.insert(ExecutionState::ROOT_STACK_ID);

        self.state.execution_heads = heads;

        let ip: InstructionPointer = ((self.state.timestamp.slide + 1) as u16, 0);
        self.state.stack_mut(ExecutionState::ROOT_STACK_ID).ip = ip;
        self.state.timestamp.slide += 1;
        self.state.timestamp.time = 0.0;
    }

    pub(crate) async fn execute_one(&mut self, stack_idx: usize) -> ExecSingle {
        self.pending_error_stack_idx = None;

        if let Err(err) = self.memory_checker.tick() {
            self.pending_error_stack_idx = Some(stack_idx);
            return ExecSingle::Error(err);
        }

        let ip = self.state.stack(stack_idx).ip;
        let section_idx = ip.0 as usize;
        let instr_idx = ip.1 as usize;

        let instr = self.bytecode.sections[section_idx].instructions[instr_idx].clone();

        self.state.stack_mut(stack_idx).ip = (section_idx as u16, (instr_idx + 1) as u32);

        let ret = self.execute_instr(section_idx, stack_idx, instr).await;
        if matches!(ret, ExecSingle::Error(_)) && self.pending_error_stack_idx.is_none() {
            self.pending_error_stack_idx = Some(stack_idx);
        }
        ret
    }

    #[inline]
    async fn execute_instr(
        &mut self,
        section_idx: usize,
        stack_idx: usize,
        instr: Instruction,
    ) -> ExecSingle {
        match instr {
            // ----- push constants -----
            Instruction::PushInt { index } => {
                let val = self.bytecode.sections[section_idx].int_pool[index as usize];
                self.state.stack_mut(stack_idx).push(Value::Integer(val));
            }
            Instruction::PushFloat { index } => {
                let val = self.bytecode.sections[section_idx].float_pool[index as usize];
                self.state.stack_mut(stack_idx).push(Value::Float(val));
            }
            Instruction::PushImaginary { index } => {
                let im = self.bytecode.sections[section_idx].float_pool[index as usize];
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::Complex { re: 0.0, im });
            }
            Instruction::PushChar { char: c } => {
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::String(c.to_string()));
            }
            Instruction::PushString { index } => {
                let s = self.bytecode.sections[section_idx].string_pool[index as usize].clone();
                self.state.stack_mut(stack_idx).push(Value::String(s));
            }
            Instruction::PushEmptyMap => {
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::Map(Rc::new(Map::new())));
            }
            Instruction::PushEmptyVector => {
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::List(Rc::new(List::new())));
            }

            // ----- variable promotion -----
            Instruction::ConvertVar {} => {
                self.state.promote_to_var(stack_idx);
            }
            Instruction::ConvertMesh { .. } => {
                self.state.promote_to_leader(stack_idx, LeaderKind::Mesh);
            }
            Instruction::ConvertState { .. } => {
                self.state.promote_to_leader(stack_idx, LeaderKind::State);
            }
            Instruction::ConvertParam { .. } => {
                self.state.promote_to_leader(stack_idx, LeaderKind::Param);
            }

            // ----- stack reads -----
            Instruction::PushCopy {
                stack_delta,
                mutable,
                pop_tos,
            } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();
                let resolved = if mutable {
                    // want to keep the nested layers of lvalue
                    val.force_elide_lvalue()
                } else {
                    val.elide_lvalue_rec()
                };
                if pop_tos {
                    self.state.stack_mut(stack_idx).pop();
                }
                self.state.stack_mut(stack_idx).push(resolved);
            }
            Instruction::PushLvalue {
                stack_delta,
                force_ephemeral,
            } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();
                let rc = match val {
                    Value::Lvalue(rc) => rc,
                    Value::WeakLvalue(weak) => weak.upgrade().unwrap(),
                    _ => panic!("PushLvalue: not an lvalue at delta {}", stack_delta),
                };
                if force_ephemeral {
                    // keep a strong ref alive for the duration of the section
                    self.state.ephemeral_pool.push(rc.clone());
                }
                // always push a non-owning weak ref
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::WeakLvalue(RcValue::downgrade(&rc)));
            }
            Instruction::PushDereference { stack_delta } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();
                let resolved = resolve_dereference(&val);
                self.state.stack_mut(stack_idx).push(resolved);
            }
            Instruction::PushStateful { stack_delta } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();
                let leader_cell_rc = match val.as_lvalue_rc() {
                    Some(rc) => rc,
                    None => {
                        return ExecSingle::Error(ExecutorError::type_error(
                            "state or param variable",
                            val.type_name(),
                        ));
                    }
                };
                if !matches!(&*leader_cell_rc.borrow(), Value::Leader(_)) {
                    return ExecSingle::Error(ExecutorError::type_error(
                        "leader",
                        leader_cell_rc.borrow().type_name(),
                    ));
                }
                let stateful = Stateful {
                    roots: vec![leader_cell_rc.clone()],
                    root: StatefulNode::LeaderRef(leader_cell_rc),
                };
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::Stateful(stateful));
            }

            // ----- labels -----
            Instruction::BufferLabelOrAttribute { string_index } => {
                self.state
                    .stack_mut(stack_idx)
                    .label_buffer
                    .push(string_index);
            }

            // ----- closures -----
            Instruction::MakeLambda {
                capture_count,
                prototype_index,
            } => {
                self.exec_make_lambda(stack_idx, section_idx, capture_count, prototype_index);
            }
            Instruction::MakeAnim {
                capture_count,
                prototype_index,
            } => {
                self.exec_make_anim(stack_idx, section_idx, capture_count, prototype_index);
            }
            Instruction::MakeOperator => {
                let stack = self.state.stack_mut(stack_idx);
                let val = stack.pop();
                match val {
                    // lambda is Rc<Lambda>, so Operator(rc) is a cheap pointer copy
                    Value::Lambda(rc) => stack.push(Value::Operator(Operator(rc))),
                    _ => {
                        return ExecSingle::Error(ExecutorError::type_error(
                            "lambda",
                            val.type_name(),
                        ));
                    }
                }
            }

            // ----- invocation -----
            Instruction::LambdaInvoke {
                stateful,
                labeled,
                num_args,
            } => {
                return self
                    .exec_lambda_invoke(stack_idx, section_idx, stateful, labeled, num_args)
                    .await;
            }
            Instruction::OperatorInvoke {
                stateful,
                labeled,
                num_args,
            } => {
                return self
                    .exec_operator_invoke(stack_idx, section_idx, stateful, labeled, num_args)
                    .await;
            }
            Instruction::ConvertToLiveOperator => {
                return self.exec_convert_to_live_operator(stack_idx);
            }

            // ----- control flow -----
            Instruction::Jump { section, to } => {
                self.state.stack_mut(stack_idx).ip = (section, to);
            }
            Instruction::ConditionalJump { section, to } => {
                let val = self.state.stack_mut(stack_idx).pop();
                let val = match val.elide_wrappers(self).await {
                    Ok(v) => v,
                    Err(e) => return ExecSingle::Error(e),
                };
                match val.check_truthy() {
                    Ok(true) => {
                        self.state.stack_mut(stack_idx).ip = (section, to);
                    }
                    Ok(false) => {}
                    Err(e) => return ExecSingle::Error(e),
                }
            }
            Instruction::Return { stack_delta } => {
                return self.exec_return(stack_idx, stack_delta);
            }
            Instruction::Pop { count } => {
                self.state.stack_mut(stack_idx).pop_n(count as usize);
            }

            // ----- native -----
            Instruction::NativeInvoke { index, arg_count } => {
                return self.exec_native_invoke(stack_idx, index, arg_count).await;
            }

            // ----- play -----
            Instruction::Play => {
                return self.exec_play(stack_idx);
            }

            // ----- unary -----
            Instruction::Negate => {
                let val = self.state.stack_mut(stack_idx).pop();
                match self.exec_negate(val).await {
                    Ok(v) => self.state.stack_mut(stack_idx).push(v),
                    Err(e) => return ExecSingle::Error(e),
                }
            }
            Instruction::Not => {
                let val = match self
                    .state
                    .stack_mut(stack_idx)
                    .pop()
                    .elide_wrappers(self)
                    .await
                {
                    Ok(val) => val,
                    Err(e) => return ExecSingle::Error(e),
                };
                match val.check_truthy() {
                    Ok(truthy) => {
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::Integer(!truthy as i64));
                    }
                    Err(e) => return ExecSingle::Error(e),
                }
            }

            // ----- subscript / attribute -----
            Instruction::Subscript { mutable } => {
                return self.exec_subscript(stack_idx, mutable);
            }
            Instruction::Attribute {
                mutable,
                string_index,
            } => {
                return self.exec_attribute(stack_idx, section_idx, mutable, string_index);
            }

            // ----- binary -----
            Instruction::Add => return self.exec_binary_op(stack_idx, BinOp::Add).await,
            Instruction::Sub => return self.exec_binary_op(stack_idx, BinOp::Sub).await,
            Instruction::Mul => return self.exec_binary_op(stack_idx, BinOp::Mul).await,
            Instruction::Div => return self.exec_binary_op(stack_idx, BinOp::Div).await,
            Instruction::Power => return self.exec_binary_op(stack_idx, BinOp::Power).await,
            Instruction::Lt => return self.exec_binary_op(stack_idx, BinOp::Lt).await,
            Instruction::Le => return self.exec_binary_op(stack_idx, BinOp::Le).await,
            Instruction::Gt => return self.exec_binary_op(stack_idx, BinOp::Gt).await,
            Instruction::Ge => return self.exec_binary_op(stack_idx, BinOp::Ge).await,
            Instruction::Eq => return self.exec_binary_op(stack_idx, BinOp::Eq).await,
            Instruction::Ne => return self.exec_binary_op(stack_idx, BinOp::Ne).await,
            Instruction::IntDiv => return self.exec_binary_op(stack_idx, BinOp::IntDiv).await,
            Instruction::In => return self.exec_binary_op(stack_idx, BinOp::In).await,
            Instruction::Assign => return self.exec_assign(stack_idx),
            Instruction::AppendAssign => return self.exec_append_assign(stack_idx),
            Instruction::Append => return self.exec_append(stack_idx),

            Instruction::EndOfExecutionHead => {
                self.finish_execution_head(stack_idx);
                return ExecSingle::EndOfHead;
            }
        }

        ExecSingle::Continue
    }

    pub(crate) fn take_error_stack_idx(&mut self, fallback_stack_idx: usize) -> usize {
        self.pending_error_stack_idx
            .take()
            .unwrap_or(fallback_stack_idx)
    }

    pub(crate) fn runtime_error_span(&self, stack_idx: usize) -> Span8 {
        let mut fallback_span = self.current_instruction_span(stack_idx);
        let mut root_span = None;

        for frame in self.recover_call_stack(stack_idx) {
            let Some(span) = self.span_for_next_ip(frame.next_ip) else {
                continue;
            };
            fallback_span = span.clone();
            if self.bytecode.sections[frame.next_ip.0 as usize]
                .flags
                .is_root_module
            {
                root_span = Some(span);
            }
        }

        root_span.unwrap_or(fallback_span)
    }

    fn current_instruction_span(&self, stack_idx: usize) -> Span8 {
        self.span_for_next_ip(self.state.stack(stack_idx).ip)
            .unwrap_or_else(|| {
                let ip = self.state.stack(stack_idx).ip;
                self.annotation_span(ip)
            })
    }

    fn annotation_span(&self, ip: InstructionPointer) -> Span8 {
        let raw = self.bytecode.sections[ip.0 as usize].annotations[ip.1 as usize]
            .source_loc
            .clone();
        normalized_span(raw)
    }

    fn span_for_next_ip(&self, next_ip: InstructionPointer) -> Option<Span8> {
        let instr_idx = next_ip.1.checked_sub(1)? as usize;
        let raw = self.bytecode.sections[next_ip.0 as usize]
            .annotations
            .get(instr_idx)?
            .source_loc
            .clone();
        Some(normalized_span(raw))
    }

    fn recover_call_stack(&self, stack_idx: usize) -> std::vec::IntoIter<RecoveredFrame> {
        let mut frames = Vec::new();
        let mut cursor = Some(stack_idx);

        while let Some(idx) = cursor {
            let stack = self.state.stack(idx);
            frames.push(RecoveredFrame {
                stack_idx: idx,
                next_ip: stack.ip,
            });
            frames.extend(
                stack
                    .call_stack
                    .iter()
                    .rev()
                    .copied()
                    .map(|next_ip| RecoveredFrame {
                        stack_idx: idx,
                        next_ip,
                    }),
            );
            cursor = stack.trace_parent_idx;
        }

        frames.into_iter()
    }
}

#[derive(Clone, Copy)]
struct RecoveredFrame {
    #[allow(dead_code)]
    stack_idx: usize,
    next_ip: InstructionPointer,
}

fn normalized_span(raw: Span8) -> Span8 {
    if raw.is_empty() {
        raw.start..raw.end + 1
    } else {
        raw
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// dereference: given an lvalue pointing to a leader, return the follower value.
/// for non-leader lvalues, return the inner value.
fn resolve_dereference(val: &Value) -> Value {
    let rc = match val {
        Value::Lvalue(rc) => rc.borrow().clone(),
        Value::WeakLvalue(weak) => match weak.upgrade() {
            Some(rc) => rc.borrow().clone(),
            None => return Value::Nil,
        },
        other => return other.clone(),
    };
    match rc {
        Value::Leader(leader) => leader.follower_rc.borrow().clone(),
        other => other,
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Power,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    IntDiv,
    In,
}
