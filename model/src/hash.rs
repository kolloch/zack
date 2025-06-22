use serde::Serialize;

pub trait Hashable {
    fn build_hash(&self, hasher: &mut blake3::Hasher);
    fn hash(&self) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();
        self.build_hash(&mut hasher);
        hasher.finalize()
    }
}

impl<S: Serialize> Hashable for S {
    fn build_hash(&self, hasher: &mut blake3::Hasher) {
        serde_json::to_writer(hasher, &self).unwrap()
    }
}
