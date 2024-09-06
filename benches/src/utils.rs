use rand::RngCore;
use std::{
    env,
    path::PathBuf,
};

/// Reimplementation of `tempdir::TempDir` that allows creating a new
/// instance without actually creating a new directory on the filesystem.
/// This is needed since rocksdb requires empty directory for checkpoints.
pub struct ShallowTempDir {
    path: PathBuf,
}

impl Default for ShallowTempDir {
    fn default() -> Self {
        Self::new()
    }
}

impl ShallowTempDir {
    /// Creates a random directory.
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let mut path = env::temp_dir();
        path.push(format!("fuel-core-shallow-{}", rng.next_u64()));
        Self { path }
    }

    /// Returns the path of the directory.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for ShallowTempDir {
    fn drop(&mut self) {
        // Ignore errors
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
