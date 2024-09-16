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

const DB_CLEAN_UP_ENV_VAR: &str = "DB_CLEAN_UP";

impl Drop for ShallowTempDir {
    fn drop(&mut self) {
        let default_db_clean_up = true;
        // Check if DB_CLEAN_UP is set and correctly parsed to a boolean, defaulting to true
        let should_clean_up = env::var(DB_CLEAN_UP_ENV_VAR)
            .map_or(default_db_clean_up, |v| {
                v.parse::<bool>().unwrap_or(default_db_clean_up)
            });

        if should_clean_up {
            if let Err(e) = std::fs::remove_dir_all(&self.path) {
                if !std::thread::panicking() {
                    panic!("Failed to remove ShallowTempDir: {}", e);
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    fn shallow_temp_dir__drops_if_env_var_is_set() {
        // given
        env::set_var(DB_CLEAN_UP_ENV_VAR, "true");
        let path;
        {
            let dir = ShallowTempDir::new();
            path = dir.path().clone();
            std::fs::create_dir_all(&path).expect("Failed to create temp directory");
        }
        // when: out of scope, dropped

        // then
        assert!(!path.exists());

        // clean up
        env::remove_var(DB_CLEAN_UP_ENV_VAR);
    }

    fn shallow_temp_dir__does_not_drop_if_env_var_is_set() {
        // given
        env::set_var(DB_CLEAN_UP_ENV_VAR, "false");
        let path;
        {
            let dir = ShallowTempDir::new();
            path = dir.path().clone();
            std::fs::create_dir_all(&path).expect("Failed to create temp directory");
        }

        // when: out of scope, not dropped

        // then
        assert!(path.exists());
        // clean up manually
        std::fs::remove_dir_all(path).unwrap();
        env::remove_var(DB_CLEAN_UP_ENV_VAR);
    }

    fn shallow_temp_dir__drops_if_env_var_is_not_set() {
        // given
        let path;
        {
            let dir = ShallowTempDir::new();
            path = dir.path().clone();
            std::fs::create_dir_all(&path).expect("Failed to create temp directory");
        }
        // when: out of scope, dropped

        // then
        assert!(!path.exists());
    }

    fn shallow_temp_dir__drops_if_env_var_malformed() {
        // given
        env::set_var(DB_CLEAN_UP_ENV_VAR, "bing_bong");
        let path;
        {
            let dir = ShallowTempDir::new();
            path = dir.path().clone();
            std::fs::create_dir_all(&path).expect("Failed to create temp directory");
        }
        // when: out of scope, dropped

        // then
        assert!(!path.exists());

        // clean up
        env::remove_var(DB_CLEAN_UP_ENV_VAR);
    }

    fn shallow_temp_dir__panics_while_dropping_if_not_panicking() {
        // given
        env::set_var(DB_CLEAN_UP_ENV_VAR, "true");

        let result = std::panic::catch_unwind(|| {
            let _ = ShallowTempDir::new();
            // when: out of scope, tries to drop
            // it will panic when trying to drop, since there
            // are no other panics
        });

        // then
        assert!(result.is_err());

        // clean up
        env::remove_var(DB_CLEAN_UP_ENV_VAR);
    }

    #[test]
    fn test_shallow_temp_dir_behaviour() {
        // run tests sequentially to avoid conflicts due to env var usage
        shallow_temp_dir__drops_if_env_var_is_set();
        shallow_temp_dir__does_not_drop_if_env_var_is_set();
        shallow_temp_dir__drops_if_env_var_is_not_set();
        shallow_temp_dir__drops_if_env_var_malformed();
        shallow_temp_dir__panics_while_dropping_if_not_panicking();
    }
}
