//! `niao_nfs` — high-level filesystem helpers: tree copy/move, atomic writes,
//! temp files/dirs, disk usage, and trash (~`shutil` + `tempfile` + `send2trash`).

pub mod atomic;
pub mod copy;
pub mod temp;
pub mod trash;
pub mod tree;
pub mod util;

pub use atomic::{write_atomic, write_bytes_atomic, AtomicWriteOpts};
pub use copy::{
    copy2, copy_file, copy_mode, copy_stat, copy_tree, copy_tree_opts_default, copyfile, CopyOpts,
    CopyTreeOpts,
};
pub use temp::{mkstemp, mktemp, temp_dir_path, TempDirGuard, TempFileGuard, TempOpts};
pub use trash::{trash_all, trash_path};
pub use tree::{
    common_prefix, disk_usage, move_path, rmtree, tree_size, walk, DiskUsage, RmTreeOpts,
    WalkEntry, WalkOpts,
};
pub use util::{samefile, which};
