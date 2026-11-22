//! A pool of strings that are built again and again (texture paths each
//! frame): the first build allocates, every later one only looks it up.

use std::cell::RefCell;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use rustc_hash::FxHashSet;

static POOL: LazyLock<Mutex<FxHashSet<Arc<str>>>> = LazyLock::new(Mutex::default);

thread_local! {
    /// Where `build` writes, kept between calls so it allocates once.
    static BUFFER: RefCell<String> = const { RefCell::new(String::new()) };
}

/// The pooled copy of what `build` writes. `build` must not intern itself.
pub fn intern(build: impl FnOnce(&mut String)) -> Arc<str> {
    BUFFER.with_borrow_mut(|buffer| {
        buffer.clear();
        build(buffer);
        let mut pool = POOL.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(pooled) = pool.get(buffer.as_str()) {
            return Arc::clone(pooled);
        }
        let pooled: Arc<str> = Arc::from(buffer.as_str());
        pool.insert(Arc::clone(&pooled));
        pooled
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_text_is_the_same_allocation() {
        let a = intern(|s| s.push_str("minecraft:entity/equipment/humanoid/iron"));
        let b = intern(|s| {
            s.push_str("minecraft:entity/equipment/");
            s.push_str("humanoid/iron");
        });
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(&*a, "minecraft:entity/equipment/humanoid/iron");
        let c = intern(|s| s.push_str("minecraft:entity/equipment/humanoid/gold"));
        assert!(!Arc::ptr_eq(&a, &c));
    }
}
