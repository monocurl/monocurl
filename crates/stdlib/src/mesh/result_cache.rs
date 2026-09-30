//! results of pure, expensive natives by their inputs. a `Shader` texture or
//! a sampled graph is a function of its callback (body, captures, defaults)
//! and its numbers, and a scene that is replayed or scrubbed asks for the
//! same frames again; keeping the result (and, through its identity, the
//! renderer's upload) makes that free. keyed by an exact encoding of the
//! arguments, bounded by bytes with the least recently used going first, and
//! dropped with the executor's bytecode generation since lambda bodies are
//! named by instruction pointer. callbacks must be ones the kernel tier can
//! compile, which is what guarantees they read nothing but their arguments

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use executor::{
    executor::Executor,
    heap::with_heap,
    value::{Value, container::HashableKey},
};
use geo::mesh::{Mesh, PixelTexture};

const MAX_BYTES: usize = 128 << 20;

/// inputs larger than this are not worth encoding
const MAX_KEY_WORDS: usize = 4096;

/// the encoded arguments of one call. meshes are keyed by identity and kept
/// alive by the key, so the identity cannot be reused while it is cached
#[derive(Clone)]
pub(super) struct Key {
    words: Vec<u64>,
    meshes: Vec<Arc<Mesh>>,
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.words == other.words
    }
}

impl Eq for Key {}

impl std::hash::Hash for Key {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.words.hash(state);
    }
}

#[derive(Clone)]
pub(super) enum Cached {
    Texture(Arc<PixelTexture>),
    Value(Value),
}

impl Cached {
    fn bytes(&self) -> usize {
        match self {
            Cached::Texture(texture) => texture.rgba.len(),
            Cached::Value(value) => value_bytes(value),
        }
    }
}

fn mesh_bytes(mesh: &Mesh) -> usize {
    mesh.tris.len() * std::mem::size_of::<geo::mesh::Tri>()
        + mesh.lins.len() * std::mem::size_of::<geo::mesh::Lin>()
        + mesh.dots.len() * std::mem::size_of::<geo::mesh::Dot>()
}

fn value_bytes(value: &Value) -> usize {
    match value {
        Value::Mesh(mesh) => mesh_bytes(mesh),
        Value::List(list) => {
            let elements: Vec<Value> = with_heap(|heap| {
                list.elements()
                    .iter()
                    .map(|key| heap.get(key.key()).clone())
                    .collect()
            });
            elements.iter().map(value_bytes).sum::<usize>() + 64
        }
        _ => 64,
    }
}

#[derive(Default)]
struct Cache {
    entries: HashMap<Key, (Cached, u64)>,
    bytes: usize,
    clock: u64,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::default();
}

/// the key of the native call whose `arity` arguments are on top of the
/// stack, or `None` when an argument cannot be keyed: a callback the kernel
/// tier cannot compile, a live value, anything stateful. `tag` tells natives
/// apart
pub(super) fn key(
    executor: &mut Executor,
    stack_idx: usize,
    arity: usize,
    tag: u64,
) -> Option<Key> {
    let mut key = Key {
        words: vec![executor.bytecode_generation(), tag],
        meshes: Vec::new(),
    };
    let args: Vec<Value> = (1..=arity)
        .rev()
        .map(|back| {
            executor
                .state
                .stack(stack_idx)
                .read_at(-(back as i32))
                .clone()
        })
        .collect();
    for arg in &args {
        encode(executor, arg, &mut key)?;
    }
    Some(key)
}

fn encode(executor: &mut Executor, value: &Value, key: &mut Key) -> Option<()> {
    if key.words.len() > MAX_KEY_WORDS {
        return None;
    }
    let out = &mut key.words;
    match value {
        Value::Nil => out.push(0),
        Value::Integer(n) => out.extend([1, *n as u64]),
        Value::Float(f) => out.extend([2, f.to_bits()]),
        Value::String(s) => {
            out.extend([3, s.len() as u64]);
            out.extend(s.as_bytes().chunks(8).map(|chunk| {
                let mut word = [0u8; 8];
                word[..chunk.len()].copy_from_slice(chunk);
                u64::from_le_bytes(word)
            }));
        }
        Value::List(list) => {
            out.extend([4, list.len() as u64]);
            let elements: Vec<Value> = with_heap(|heap| {
                list.elements()
                    .iter()
                    .map(|key| heap.get(key.key()).clone())
                    .collect()
            });
            for element in &elements {
                encode(executor, element, key)?;
            }
        }
        Value::Lambda(lambda) => {
            if !executor.lambda_is_pure(lambda) {
                return None;
            }
            out.extend([
                5,
                u64::from(lambda.ip.0),
                u64::from(lambda.ip.1),
                lambda.captures.len() as u64,
                lambda.defaults.len() as u64,
            ]);
            for value in lambda.captures.iter().chain(&lambda.defaults) {
                encode(executor, value, key)?;
            }
        }
        Value::Mesh(mesh) => {
            out.extend([6, Arc::as_ptr(mesh) as usize as u64, mesh.version()]);
            key.meshes.push(Arc::clone(mesh));
        }
        Value::Lvalue(reference) => {
            let inner = with_heap(|heap| heap.get(reference.key()).clone());
            encode(executor, &inner, key)?;
        }
        // palettes: keyed in insertion order, so two equal maps built in a
        // different order miss each other, which only costs a re-run
        Value::Map(map) => {
            out.extend([7, map.len() as u64]);
            for map_key in &map.insertion_order {
                encode_key(map_key, &mut key.words)?;
                let value = map.get(map_key)?;
                let inner = with_heap(|heap| heap.get(value.key()).clone());
                encode(executor, &inner, key)?;
            }
        }
        _ => return None,
    }
    Some(())
}

fn encode_key(map_key: &HashableKey, out: &mut Vec<u64>) -> Option<()> {
    match map_key {
        HashableKey::Integer(n) => out.extend([1, *n as u64]),
        HashableKey::Float(bits) => out.extend([2, *bits]),
        HashableKey::String(s) => {
            out.extend([3, s.len() as u64]);
            out.extend(s.as_bytes().chunks(8).map(|chunk| {
                let mut word = [0u8; 8];
                word[..chunk.len()].copy_from_slice(chunk);
                u64::from_le_bytes(word)
            }));
        }
        HashableKey::List(_) => return None,
    }
    Some(())
}

pub(super) fn get(key: &Key) -> Option<Cached> {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clock += 1;
        let clock = cache.clock;
        let (cached, used) = cache.entries.get_mut(key)?;
        *used = clock;
        Some(cached.clone())
    })
}

pub(super) fn insert(key: Key, cached: Cached) {
    let bytes = cached.bytes()
        + key
            .meshes
            .iter()
            .map(|mesh| mesh_bytes(mesh))
            .sum::<usize>();
    if bytes > MAX_BYTES / 4 {
        return;
    }
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clock += 1;
        let clock = cache.clock;
        if let Some((old, _)) = cache.entries.insert(key, (cached, clock)) {
            cache.bytes -= old.bytes();
        }
        cache.bytes += bytes;
        while cache.bytes > MAX_BYTES {
            let Some(oldest) = cache
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some((old, _)) = cache.entries.remove(&oldest) {
                cache.bytes -= old.bytes()
                    + oldest
                        .meshes
                        .iter()
                        .map(|mesh| mesh_bytes(mesh))
                        .sum::<usize>();
            }
        }
    })
}

/// the key of the native call on top of the stack and, when its result is
/// cached, the result
pub(super) fn lookup(
    executor: &mut Executor,
    stack_idx: usize,
    arity: usize,
    tag: u64,
) -> (Option<Key>, Option<Value>) {
    let key = key(executor, stack_idx, arity, tag);
    let hit = match key.as_ref().and_then(get) {
        Some(Cached::Value(value)) => Some(value),
        _ => None,
    };
    (key, hit)
}

pub(super) fn remember(key: Option<Key>, value: &Value) {
    if let Some(key) = key {
        insert(key, Cached::Value(value.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texture(bytes: usize) -> Cached {
        Cached::Texture(Arc::new(PixelTexture::new(
            bytes as u32 / 4,
            1,
            vec![0; bytes],
        )))
    }

    fn plain_key(word: u64) -> Key {
        Key {
            words: vec![word],
            meshes: Vec::new(),
        }
    }

    #[test]
    fn the_cache_evicts_least_recently_used_past_its_budget() {
        CACHE.with(|cache| *cache.borrow_mut() = Cache::default());
        let chunk = MAX_BYTES / 4;
        for i in 0..4u64 {
            insert(plain_key(i), texture(chunk));
        }
        assert!(get(&plain_key(0)).is_some());
        insert(plain_key(4), texture(chunk));
        assert!(get(&plain_key(0)).is_some());
        assert!(get(&plain_key(1)).is_none());
        assert!(CACHE.with(|cache| cache.borrow().bytes) <= MAX_BYTES);
    }
}
