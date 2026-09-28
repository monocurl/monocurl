//! shader textures by their inputs. a `Shader` mesh is a pure function of its
//! colour lambda (body, captures, defaults) and its domain, and a scene that
//! is replayed or scrubbed asks for the same frames again; keeping the
//! pixels (and, through the texture's identity, the renderer's upload) makes
//! that free. keyed by an exact encoding of the inputs, bounded by bytes with
//! the least recently used going first, and dropped with the bytecode
//! generation since lambda bodies are named by instruction pointer

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use executor::{heap::with_heap, value::Value};
use geo::mesh::PixelTexture;

const MAX_BYTES: usize = 96 << 20;

/// inputs larger than this are not worth encoding
const MAX_KEY_WORDS: usize = 4096;

pub(super) type Key = Vec<u64>;

#[derive(Default)]
struct Cache {
    entries: HashMap<Key, (Arc<PixelTexture>, u64)>,
    bytes: usize,
    clock: u64,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::default();
}

/// the exact encoding of a shader's inputs, or `None` when they hold
/// something a texture cannot be keyed by (a mesh, a live value)
pub(super) fn key(generation: u64, color_at: &Value, params: &[u64]) -> Option<Key> {
    let mut words = vec![generation];
    words.extend_from_slice(params);
    encode(color_at, &mut words)?;
    Some(words)
}

fn encode(value: &Value, out: &mut Vec<u64>) -> Option<()> {
    if out.len() > MAX_KEY_WORDS {
        return None;
    }
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
                encode(element, out)?;
            }
        }
        Value::Lambda(lambda) => {
            out.extend([
                5,
                u64::from(lambda.ip.0),
                u64::from(lambda.ip.1),
                lambda.captures.len() as u64,
                lambda.defaults.len() as u64,
            ]);
            for value in lambda.captures.iter().chain(&lambda.defaults) {
                encode(value, out)?;
            }
        }
        Value::Lvalue(reference) => {
            let inner = with_heap(|heap| heap.get(reference.key()).clone());
            encode(&inner, out)?;
        }
        _ => return None,
    }
    Some(())
}

pub(super) fn get(key: &Key) -> Option<Arc<PixelTexture>> {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clock += 1;
        let clock = cache.clock;
        let (texture, used) = cache.entries.get_mut(key)?;
        *used = clock;
        Some(Arc::clone(texture))
    })
}

pub(super) fn insert(key: Key, texture: Arc<PixelTexture>) {
    let bytes = texture.rgba.len();
    if bytes > MAX_BYTES / 4 {
        return;
    }
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clock += 1;
        let clock = cache.clock;
        if let Some((old, _)) = cache.entries.insert(key, (texture, clock)) {
            cache.bytes -= old.rgba.len();
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
                cache.bytes -= old.rgba.len();
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texture(bytes: usize) -> Arc<PixelTexture> {
        Arc::new(PixelTexture::new(bytes as u32 / 4, 1, vec![0; bytes]))
    }

    #[test]
    fn keys_encode_lambdas_structurally_and_refuse_unencodable_captures() {
        let lambda = executor::value::lambda::Lambda {
            ip: (2, 7),
            captures: smallvec::smallvec![Value::Float(1.5), Value::Integer(3)],
            required_args: 2,
            defaults: smallvec::smallvec![],
            reference_args: vec![false, false],
            arg_names: vec![],
        };
        let a = key(1, &Value::Lambda(std::rc::Rc::new(lambda.clone())), &[9]).unwrap();
        let b = key(1, &Value::Lambda(std::rc::Rc::new(lambda)), &[9]).unwrap();
        assert_eq!(a, b);
        let mesh = super::super::helpers::mesh_from_parts(vec![], vec![], vec![]);
        assert!(key(1, &mesh, &[]).is_none());
    }

    #[test]
    fn the_cache_evicts_least_recently_used_past_its_budget() {
        CACHE.with(|cache| *cache.borrow_mut() = Cache::default());
        let chunk = MAX_BYTES / 4;
        for i in 0..4u64 {
            insert(vec![i], texture(chunk));
        }
        assert!(get(&vec![0]).is_some());
        insert(vec![4], texture(chunk));
        assert!(get(&vec![0]).is_some());
        assert!(get(&vec![1]).is_none());
        assert!(CACHE.with(|cache| cache.borrow().bytes) <= MAX_BYTES);
    }
}
