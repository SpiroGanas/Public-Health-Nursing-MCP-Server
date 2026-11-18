//! The `/data` sandbox.
//!
//! Tools may read only inside the nurse's mounted data folder and write only
//! inside its output folder. Every path from a tool argument is resolved here:
//!
//! * absolute paths must already be inside the data folder;
//! * `..` components are rejected outright;
//! * the resolved path is canonicalized (following symbolic links) and must
//!   still be inside the canonical data folder, so a symlink cannot escape.
//!
//! Tool descriptions and the model call the data folder `/data`, as it is in the
//! container. When the server runs natively (no Docker), `/data/...` and
//! `\data\...` are mapped to the real data folder on every platform, and on
//! Windows the folder's ordinary path (`C:\Users\...`) is accepted as well as
//! the `\\?\` form that canonicalization returns.

use std::path::{Component, Path, PathBuf};

use phn_core::{PhnError, Result};

/// Resolved data and output folders.
#[derive(Debug, Clone)]
pub struct Sandbox {
    data_root: PathBuf,
    /// `data_root` without a Windows verbatim (`\\?\`) prefix; equal to it elsewhere.
    data_root_plain: PathBuf,
    output_dir: PathBuf,
}

impl Sandbox {
    /// Create a sandbox. The data folder must exist.
    pub fn new(data_dir: &Path, output_dir: &Path) -> Result<Self> {
        let data_root = data_dir.canonicalize().map_err(|_| {
            PhnError::Config(format!(
                "the data folder {} does not exist. With Docker, mount a folder at /data \
                 (docker run -v <your folder>:/data ...); without Docker, set PHN_DATA_DIR \
                 to a folder that exists.",
                data_dir.display()
            ))
        })?;
        let data_root_plain = strip_verbatim_prefix(&data_root);
        Ok(Self {
            data_root,
            data_root_plain,
            output_dir: output_dir.to_path_buf(),
        })
    }

    /// The canonical data folder.
    pub fn root(&self) -> &Path {
        &self.data_root
    }

    /// Resolve an existing file or folder for reading.
    pub fn resolve_existing(&self, user_path: &str) -> Result<PathBuf> {
        let joined = self.join_checked(user_path)?;
        let canonical = joined.canonicalize().map_err(|_| {
            PhnError::NotFound(format!(
                "'{user_path}' was not found in the data folder. Use read_local_file with \
                 action 'list' to see available files."
            ))
        })?;
        self.ensure_inside(&canonical, user_path)?;
        Ok(canonical)
    }

    /// The output folder, created on first use. It must be inside the data
    /// folder or explicitly configured.
    pub fn output_dir(&self) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.output_dir).map_err(|e| {
            PhnError::Config(format!(
                "cannot create the output folder {}: {e}",
                self.output_dir.display()
            ))
        })?;
        Ok(self.output_dir.canonicalize()?)
    }

    /// A path for a new file in the output folder. The name is sanitized and,
    /// if a file with that name exists, a numeric suffix is added so nothing is
    /// overwritten.
    pub fn new_output_file(&self, file_name: &str) -> Result<PathBuf> {
        let dir = self.output_dir()?;
        let clean = sanitize_file_name(file_name);
        let candidate = dir.join(&clean);
        // `symlink_metadata` also sees a dangling symbolic link, which `exists` reports as absent
        // and which a write would follow to wherever it points.
        if candidate.symlink_metadata().is_err() {
            return Ok(candidate);
        }
        let (stem, ext) = match clean.rsplit_once('.') {
            Some((s, e)) => (s.to_string(), format!(".{e}")),
            None => (clean.clone(), String::new()),
        };
        for n in 2..10_000 {
            let p = dir.join(format!("{stem}-{n}{ext}"));
            if p.symlink_metadata().is_err() {
                return Ok(p);
            }
        }
        Err(PhnError::Internal(
            "too many files with the same name".into(),
        ))
    }

    /// Path relative to the data folder, for display and citations.
    pub fn display_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.data_root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| {
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
    }

    fn join_checked(&self, user_path: &str) -> Result<PathBuf> {
        let trimmed = user_path.trim();
        let p = Path::new(trimmed);
        if p.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(PhnError::Sandbox(format!(
                "'{user_path}' contains '..'; paths must stay inside the data folder"
            )));
        }
        // The container path `/data` (either slash), checked as text so it works on Windows,
        // where `/data/x` is not an absolute path and joining it would drop the data folder.
        if let Some(rest) = strip_logical_root(trimmed) {
            let rest_path = Path::new(rest);
            if rest_path.has_root()
                || rest_path
                    .components()
                    .any(|c| matches!(c, Component::Prefix(_)))
            {
                return Err(PhnError::Sandbox(format!(
                    "'{user_path}' is outside the data folder; use a path relative to /data"
                )));
            }
            return Ok(self.data_root.join(rest_path));
        }
        if p.is_absolute() || p.has_root() {
            // Accept other absolute paths only when they point inside the real data folder.
            for root in [&self.data_root, &self.data_root_plain] {
                if let Ok(rest) = p.strip_prefix(root) {
                    return Ok(self.data_root.join(rest));
                }
            }
            return Err(PhnError::Sandbox(format!(
                "'{user_path}' is outside the data folder; use a path relative to /data"
            )));
        }
        Ok(self.data_root.join(p))
    }

    fn ensure_inside(&self, canonical: &Path, user_path: &str) -> Result<()> {
        if canonical.starts_with(&self.data_root) {
            Ok(())
        } else {
            Err(PhnError::Sandbox(format!(
                "'{user_path}' resolves outside the data folder (symbolic link?)"
            )))
        }
    }
}

/// If `path` starts with the container data folder (`/data` or `\data`, as a whole
/// component), return the rest with leading separators removed (empty for the folder itself).
fn strip_logical_root(path: &str) -> Option<&str> {
    let rest = path
        .strip_prefix("/data")
        .or_else(|| path.strip_prefix("\\data"))?;
    if rest.is_empty() {
        return Some("");
    }
    if rest.starts_with(['/', '\\']) {
        Some(rest.trim_start_matches(['/', '\\']))
    } else {
        None
    }
}

/// Remove a Windows verbatim drive prefix (`\\?\C:\...` becomes `C:\...`), which
/// `canonicalize` adds on Windows, so ordinary paths the nurse types can be compared with it.
/// Verbatim UNC paths and paths without the prefix are returned unchanged.
fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\")
        && rest.len() >= 2
        && rest.as_bytes()[1] == b':'
        && rest.as_bytes()[0].is_ascii_alphabetic()
    {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

/// Make a safe file name: keep letters, digits, `-`, `_`, `.`, and spaces
/// (as `_`); drop path separators and leading dots.
pub fn sanitize_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let mut out: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    while out.starts_with('.') {
        out.remove(0);
    }
    if out.len() > 120 {
        // Shorten the stem on a character boundary and keep the extension
        // (truncating bytes would panic inside a multi-byte character).
        let (stem, ext) = match out.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() && e.len() <= 10 => (s.to_string(), format!(".{e}")),
            _ => (out.clone(), String::new()),
        };
        let mut keep = 120usize.saturating_sub(ext.len());
        while !stem.is_char_boundary(keep.min(stem.len())) {
            keep -= 1;
        }
        out = format!("{}{ext}", &stem[..keep.min(stem.len())]);
    }
    if out.is_empty() { "output".into() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Sandbox) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join("sub")).unwrap();
        std::fs::write(data.join("sub/a.csv"), "x").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "no").unwrap();
        let sb = Sandbox::new(&data, &data.join("output")).unwrap();
        (dir, sb)
    }

    #[test]
    fn resolves_relative_and_container_paths() {
        let (_d, sb) = setup();
        assert!(sb.resolve_existing("sub/a.csv").is_ok());
        assert!(sb.resolve_existing("/data/sub/a.csv").is_ok());
        assert!(matches!(
            sb.resolve_existing("missing.csv"),
            Err(PhnError::NotFound(_))
        ));
    }

    #[test]
    fn container_root_is_matched_as_text_with_either_slash() {
        assert_eq!(strip_logical_root("/data"), Some(""));
        assert_eq!(strip_logical_root("/data/"), Some(""));
        assert_eq!(strip_logical_root("/data/sub/a.csv"), Some("sub/a.csv"));
        assert_eq!(strip_logical_root("\\data\\sub\\a.csv"), Some("sub\\a.csv"));
        assert_eq!(strip_logical_root("/data//a.csv"), Some("a.csv"));
        // Only the whole component `data` counts.
        assert_eq!(strip_logical_root("/data2/a.csv"), None);
        assert_eq!(strip_logical_root("/database.csv"), None);
        assert_eq!(strip_logical_root("data/a.csv"), None);
    }

    #[test]
    fn verbatim_drive_prefix_is_removed_and_nothing_else() {
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\C:\Users\nurse\PHN-Data")),
            PathBuf::from(r"C:\Users\nurse\PHN-Data")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\UNC\server\share")),
            PathBuf::from(r"\\?\UNC\server\share")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new("/home/nurse/PHN-Data")),
            PathBuf::from("/home/nurse/PHN-Data")
        );
    }

    #[test]
    fn container_paths_resolve_and_cannot_escape() {
        let (_d, sb) = setup();
        assert!(sb.resolve_existing("/data").unwrap().is_dir());
        assert!(sb.resolve_existing("/data/sub").unwrap().is_dir());
        assert!(matches!(
            sb.resolve_existing("/data/../secret.txt"),
            Err(PhnError::Sandbox(_))
        ));
        assert!(matches!(
            sb.resolve_existing("/data//etc/passwd"),
            Err(PhnError::NotFound(_))
        ));
        // The real data folder's own absolute path also works.
        let real = sb.root().join("sub").join("a.csv");
        assert!(sb.resolve_existing(&real.to_string_lossy()).is_ok());
    }

    #[test]
    fn blocks_traversal_absolute_and_symlink_escape() {
        let (d, sb) = setup();
        assert!(matches!(
            sb.resolve_existing("../secret.txt"),
            Err(PhnError::Sandbox(_))
        ));
        assert!(matches!(
            sb.resolve_existing("sub/../../secret.txt"),
            Err(PhnError::Sandbox(_))
        ));
        assert!(matches!(
            sb.resolve_existing("/etc/passwd"),
            Err(PhnError::Sandbox(_))
        ));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(d.path().join("secret.txt"), sb.root().join("link.txt"))
                .unwrap();
            assert!(matches!(
                sb.resolve_existing("link.txt"),
                Err(PhnError::Sandbox(_))
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_in_the_output_folder_is_never_written_through() {
        let (d, sb) = setup();
        let out = sb.output_dir().unwrap();
        let target = d.path().join("elsewhere.docx");
        std::os::unix::fs::symlink(&target, out.join("report.docx")).unwrap();
        // The target does not exist, so `exists()` would call the name free and a write would
        // create the file outside the output folder.
        let p = sb.new_output_file("report.docx").unwrap();
        assert!(p.ends_with("report-2.docx"), "{p:?}");
        assert!(!target.exists());
    }

    #[test]
    fn output_files_never_overwrite() {
        let (_d, sb) = setup();
        let a = sb.new_output_file("report.docx").unwrap();
        std::fs::write(&a, "1").unwrap();
        let b = sb.new_output_file("report.docx").unwrap();
        assert_ne!(a, b);
        assert!(b.ends_with("report-2.docx"));
        assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_file_name(".hidden name.csv"), "hidden_name.csv");
        // Long names with accented letters keep their extension and do not panic.
        let long = format!("{}.docx", "Notificación de exclusión ".repeat(10));
        let clean = sanitize_file_name(&long);
        assert!(clean.len() <= 120 && clean.ends_with(".docx"), "{clean}");
    }
}
