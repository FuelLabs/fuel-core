use clap::{
    Parser,
    Subcommand,
};
use fuel_core::combined_database::CombinedDatabase;

#[derive(Debug, Subcommand)]
pub enum Command {
    Backup(BackupArgs),
    Restore(RestoreArgs),
}

#[derive(Debug, Parser)]
pub struct BackupArgs {
    #[arg(long)]
    pub from: String,

    #[arg(long)]
    pub to: String,
}

#[derive(Debug, Parser)]
pub struct RestoreArgs {
    #[arg(long)]
    pub from: String,

    #[arg(long)]
    pub to: String,
}

pub fn exec(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Backup(args) => backup(&args.from, &args.to),
        Command::Restore(args) => restore(&args.from, &args.to),
    }
}

#[cfg(not(feature = "archive"))]
pub fn backup(db_dir: &str, backup_path: &str) -> anyhow::Result<()> {
    CombinedDatabase::backup(db_dir.as_ref(), backup_path.as_ref())?;
    Ok(())
}

#[cfg(feature = "archive")]
pub fn backup(db_dir: &str, backup_path: &str) -> anyhow::Result<()> {
    let tmp_dir = tempfile::TempDir::new()?;
    CombinedDatabase::create_full_backup(db_dir.as_ref(), tmp_dir.path())?;
    archiver::add_to_archive(tmp_dir.path(), backup_path.as_ref())?;
    Ok(())
}

#[cfg(not(feature = "archive"))]
pub fn restore(restore_to: &str, backup_path: &str) -> anyhow::Result<()> {
    CombinedDatabase::restore(restore_to.as_ref(), backup_path.as_ref())?;
    Ok(())
}

#[cfg(feature = "archive")]
pub fn restore(restore_to: &str, backup_path: &str) -> anyhow::Result<()> {
    let tmp_dir = tempfile::TempDir::new()?;
    archiver::extract_from_archive(backup_path.as_ref(), tmp_dir.path())?;
    CombinedDatabase::restore(restore_to.as_ref(), tmp_dir.path())?;
    Ok(())
}

#[cfg(feature = "archive")]
mod archiver {
    use anyhow::anyhow;
    use rayon::prelude::*;
    use std::{
        fs::{
            self,
            File,
        },
        io::{
            BufReader,
            BufWriter,
        },
        os::unix::fs::PermissionsExt,
        path::{
            Path,
            PathBuf,
        },
    };
    use tar::{
        Archive,
        Builder,
        Header,
    };

    pub fn add_to_archive(src_dir: &Path, dest_path: &Path) -> anyhow::Result<()> {
        let top_level_dirs = fs::read_dir(src_dir)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_dir() { Some(path) } else { None }
            })
            .collect::<Vec<_>>();

        let tar_paths = top_level_dirs
            .par_iter()
            .map(|dir| {
                let tar_path = dest_path.join(format!(
                    "{}.tar",
                    dir.file_name()
                        .ok_or(anyhow!("Failed to get the directory name."))?
                        .to_string_lossy()
                ));
                let writer = BufWriter::new(File::create(&tar_path)?);
                let mut tar = Builder::new(writer);

                let files = collect_files(dir)?
                    .into_par_iter()
                    .filter_map(|(rel, abs)| {
                        let metadata = abs.metadata().ok()?;
                        let mut header = Header::new_gnu();
                        header.set_path(rel).ok()?;
                        header.set_size(metadata.len());
                        header.set_mode(metadata.permissions().mode());
                        header.set_mtime(
                            metadata.modified().ok()?.elapsed().ok()?.as_secs(),
                        );
                        header.set_cksum();
                        Some((header, abs))
                    })
                    .collect::<Vec<_>>();

                for (header, path) in files {
                    tar.append(&header, File::open(path)?)?;
                }

                tar.finish()?;
                Ok::<_, anyhow::Error>(tar_path)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut final_writer = BufWriter::new(File::create(dest_path.join("db.tar"))?);
        for tar_path in tar_paths {
            let mut reader = BufReader::new(File::open(tar_path)?);
            std::io::copy(&mut reader, &mut final_writer)?;
        }

        Ok(())
    }

    pub fn extract_from_archive(src_file: &Path, dest_dir: &Path) -> anyhow::Result<()> {
        use rayon::prelude::*;

        // Step 1: Extract the main archive
        let reader = BufReader::new(File::open(src_file)?);
        Archive::new(reader).unpack(dest_dir)?;

        // Step 2: Find nested .tar files in dest_dir
        let nested_tars = std::fs::read_dir(dest_dir)?
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "tar") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // Step 3: Extract each nested tar in parallel
        nested_tars.par_iter().try_for_each(|tar_path| {
            let subdir_name = tar_path
                .file_stem()
                .ok_or_else(|| anyhow::anyhow!("Invalid tar filename"))?;
            let subdir_path = dest_dir.join(subdir_name);

            std::fs::create_dir_all(&subdir_path)?;
            let reader = BufReader::new(File::open(tar_path)?);
            Archive::new(reader).unpack(&subdir_path)?;
            Ok::<(), anyhow::Error>(())
        })?;

        Ok(())
    }

    fn collect_files(base: &Path) -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(base)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(base)?.to_path_buf();

            if path.is_file() {
                files.push((rel, path));
            } else if path.is_dir() {
                files.extend(collect_files(&path)?);
            }
        }
        Ok(files)
    }
}
