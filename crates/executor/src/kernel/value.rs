//! the value model of the kernel tier: plain data with no ties to the
//! thread-local heap, so a batch of kernel calls can run on worker threads

use std::sync::Arc;

use smallvec::SmallVec;

use crate::value::InstructionPointer;

use super::ir::Kernel;

/// elements stay inline for the short vectors that dominate numeric code, so a
/// `[x, y, z]` costs one allocation
pub type KList = SmallVec<[KVal; 4]>;

#[derive(Clone, Debug)]
pub enum KVal {
    Nil,
    Int(i64),
    Float(f64),
    List(Arc<KList>),
    /// an index into the batch's `ClosureArena`. closures are values the
    /// kernels copy constantly (every call copies its captures into the new
    /// frame), and an index copies without touching a shared reference count,
    /// which worker threads would otherwise fight over
    Closure(ClosureId),
    /// a value the tier does not model. it can be carried around and returned
    /// through a list, but any operation that inspects it faults, which hands
    /// the call back to the interpreter
    Opaque,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClosureId(pub u32);

/// a lambda value as a kernel sees it: its compiled body plus the captured and
/// default values it closes over, already converted
#[derive(Debug)]
pub struct KClosure {
    pub ip: InstructionPointer,
    pub kernel: Arc<Kernel>,
    pub captures: Box<[KVal]>,
    pub defaults: Box<[KVal]>,
}

/// every closure a batch can reach, built during conversion and shared
/// read-only by the threads that run the batch
#[derive(Debug, Default)]
pub struct ClosureArena {
    closures: Vec<KClosure>,
}

impl ClosureArena {
    pub fn push(&mut self, closure: KClosure) -> ClosureId {
        let id = ClosureId(self.closures.len() as u32);
        self.closures.push(closure);
        id
    }

    pub fn get(&self, id: ClosureId) -> &KClosure {
        &self.closures[id.0 as usize]
    }

    /// whether any placed closure fills default arguments; a call to one is
    /// a live value in the interpreter, so a single call whose result may be
    /// such a value must not run here
    pub fn any_defaults(&self) -> bool {
        self.closures
            .iter()
            .any(|closure| !closure.defaults.is_empty())
    }

    pub fn clear(&mut self) {
        self.closures.clear();
    }
}

impl KVal {
    pub fn list(elements: impl IntoIterator<Item = KVal>) -> Self {
        KVal::List(Arc::new(elements.into_iter().collect()))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            KVal::Nil => "nil",
            KVal::Int(_) => "int",
            KVal::Float(_) => "float",
            KVal::List(_) => "list",
            KVal::Closure(_) => "lambda",
            KVal::Opaque => "opaque",
        }
    }

    /// the same structural equality as `Value::values_equal`, undecidable for
    /// opaque operands
    pub fn equals(arena: &ClosureArena, a: &KVal, b: &KVal) -> Option<bool> {
        Some(match (a, b) {
            (KVal::Opaque, _) | (_, KVal::Opaque) => return None,
            (KVal::Nil, KVal::Nil) => true,
            (KVal::Int(x), KVal::Int(y)) => x == y,
            (KVal::Float(x), KVal::Float(y)) => x == y,
            (KVal::Int(x), KVal::Float(y)) => (*x as f64) == *y,
            (KVal::Float(x), KVal::Int(y)) => *x == (*y as f64),
            (KVal::List(x), KVal::List(y)) => {
                if x.len() != y.len() {
                    return Some(false);
                }
                for (a, b) in x.iter().zip(y.iter()) {
                    if !KVal::equals(arena, a, b)? {
                        return Some(false);
                    }
                }
                true
            }
            (KVal::Closure(x), KVal::Closure(y)) => x == y || arena.get(*x).ip == arena.get(*y).ip,
            _ => false,
        })
    }
}

impl KVal {
    /// equality that distinguishes `1` from `1.0` and compares floats by
    /// bits, for checking one engine against another
    pub fn strictly_equal(a: &KVal, b: &KVal) -> bool {
        match (a, b) {
            (KVal::Nil, KVal::Nil) | (KVal::Opaque, KVal::Opaque) => true,
            (KVal::Int(x), KVal::Int(y)) => x == y,
            (KVal::Float(x), KVal::Float(y)) => {
                x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
            }
            (KVal::List(x), KVal::List(y)) => {
                x.len() == y.len()
                    && x.iter()
                        .zip(y.iter())
                        .all(|(a, b)| KVal::strictly_equal(a, b))
            }
            (KVal::Closure(x), KVal::Closure(y)) => x == y,
            _ => false,
        }
    }
}

const _: () = assert!(std::mem::size_of::<KVal>() <= 24);
