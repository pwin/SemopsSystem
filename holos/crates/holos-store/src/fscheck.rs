//! What kind of filesystem a store is on, and whether its locks keep a second writer out.
//!
//! `RocksDB` admits one writer by taking a POSIX (`fcntl`) lock on the store's `LOCK` file.
//! On a local disk that lock is the kernel's, and on most network filesystems it is the
//! server's, so a second process on another machine is refused. But an NFS mount can be
//! made with its locks kept local to the client (`nolock`, which Linux reports as
//! `local_lock=all` or `local_lock=posix`): every client then grants itself the lock, and
//! two machines opening the same store each believe they are its only writer. Azure Blob
//! Storage's NFS 3.0 endpoint requires exactly that mount option. Two writers on one `RocksDB`
//! store corrupt it, quietly and late.
//!
//! [`inspect`] reads where a path is mounted from `/proc/self/mountinfo` on Linux, the one
//! platform this question has a reliable answer on; elsewhere it answers
//! [`Locking::Unknown`] and nothing is refused.

use std::path::{Path, PathBuf};

/// Whether a filesystem's locks can keep a writer on another machine out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Locking {
    /// A local filesystem, or a network one whose POSIX locks are the server's.
    Shared {
        /// The filesystem type, as the mount table names it: `ext4`, `nfs4`.
        fstype: String,
        /// Where it is mounted.
        mount_point: PathBuf,
    },
    /// A network filesystem whose POSIX locks never leave the client.
    LocalOnly {
        /// The filesystem type: `nfs` or `nfs4`.
        fstype: String,
        /// Where it is mounted.
        mount_point: PathBuf,
        /// The mount option that says so: `nolock`, `local_lock=all` or `local_lock=posix`.
        option: String,
    },
    /// No answer on this platform, or the mount table could not be read.
    Unknown,
}

impl Locking {
    /// Whether the filesystem is a network one (NFS, SMB/CIFS, or a FUSE network mount
    /// such as blobfuse), where a store pays a network round trip per synchronous write.
    #[must_use]
    pub fn is_network(&self) -> bool {
        match self {
            Self::Shared { fstype, .. } | Self::LocalOnly { fstype, .. } => is_network(fstype),
            Self::Unknown => false,
        }
    }
}

fn is_network(fstype: &str) -> bool {
    fstype.starts_with("nfs")
        || matches!(
            fstype,
            "cifs" | "smb3" | "smbfs" | "9p" | "ceph" | "glusterfs"
        )
        || fstype.starts_with("fuse.blobfuse")
        || fstype.starts_with("fuse.s3")
}

/// Where `path` is mounted, and what that means for locking.
#[must_use]
pub fn inspect(path: &Path) -> Locking {
    #[cfg(target_os = "linux")]
    {
        let Ok(table) = std::fs::read_to_string("/proc/self/mountinfo") else {
            return Locking::Unknown;
        };
        // The store directory may not exist yet; its nearest existing ancestor is on the
        // same mount it will be created on.
        let mut probe = path.to_path_buf();
        let resolved = loop {
            if let Ok(real) = probe.canonicalize() {
                break real;
            }
            if !probe.pop() {
                return Locking::Unknown;
            }
        };
        classify(&table, &resolved)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        Locking::Unknown
    }
}

/// Undoes the octal escapes `mountinfo` uses for spaces and the like (`\040`).
fn unescape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let bytes = field.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 4 <= bytes.len()
            && bytes[i + 1..i + 4]
                .iter()
                .all(|b| (b'0'..=b'7').contains(b))
        {
            let code = u8::from_str_radix(&field[i + 1..i + 4], 8).unwrap_or(b'?');
            out.push(char::from(code));
            i += 4;
        } else {
            out.push(char::from(bytes[i]));
            i += 1;
        }
    }
    out
}

/// Classifies `path` against a `mountinfo` table: the mount with the longest mount point
/// that contains it decides.
#[must_use]
pub fn classify(mountinfo: &str, path: &Path) -> Locking {
    let mut best: Option<(PathBuf, String, String)> = None;
    for line in mountinfo.lines() {
        // `36 35 98:0 /root /mnt rw,noatime shared:1 - nfs4 server:/export rw,vers=4.1,...`
        let Some((left, right)) = line.split_once(" - ") else {
            continue;
        };
        let Some(mount_point) = left.split(' ').nth(4).map(unescape).map(PathBuf::from) else {
            continue;
        };
        let mut right = right.split(' ');
        let (Some(fstype), _source, Some(options)) = (right.next(), right.next(), right.next())
        else {
            continue;
        };
        if !path.starts_with(&mount_point) {
            continue;
        }
        let deeper = best
            .as_ref()
            .is_none_or(|(m, _, _)| mount_point.components().count() >= m.components().count());
        if deeper {
            best = Some((mount_point, fstype.to_owned(), options.to_owned()));
        }
    }
    let Some((mount_point, fstype, options)) = best else {
        return Locking::Unknown;
    };
    if fstype.starts_with("nfs") {
        // `local_lock=flock` keeps only flock(2) local; RocksDB's lock is fcntl(2), which
        // still reaches the server, so that one is fine.
        let local = options
            .split(',')
            .find(|o| matches!(*o, "nolock" | "local_lock=all" | "local_lock=posix"));
        if let Some(option) = local {
            return Locking::LocalOnly {
                fstype,
                mount_point,
                option: option.to_owned(),
            };
        }
    }
    Locking::Shared {
        fstype,
        mount_point,
    }
}

/// The environment variable that lets a writable store open where [`Locking::LocalOnly`]
/// says its lock protects nothing: for a deployment that guarantees one writer some other
/// way, and accepts the risk if it does not.
pub const ALLOW_LOCAL_LOCKS: &str = "HOLOS_ALLOW_LOCAL_LOCKS";

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "\
22 1 8:1 / / rw,relatime shared:1 - ext4 /dev/sda1 rw
41 22 0:40 / /mnt/files rw,relatime shared:20 - nfs4 acct.file.core.windows.net:/acct/share rw,vers=4.1,rsize=1048576,wsize=1048576,hard,proto=tcp,timeo=600,sec=sys,local_lock=none,addr=10.0.0.4
42 22 0:41 / /mnt/blob rw,relatime shared:21 - nfs acct.blob.core.windows.net:/acct/container rw,vers=3,rsize=1048576,wsize=1048576,namlen=255,hard,nolock,proto=tcp,sec=sys,mountvers=3,local_lock=all,addr=10.0.0.5
43 22 0:42 / /mnt/anf rw,relatime shared:22 - nfs 10.0.1.4:/vol1 rw,vers=3,hard,proto=tcp,sec=sys,mountvers=3,mountproto=tcp,local_lock=none,addr=10.0.1.4
44 41 0:43 / /mnt/files/inner\\040dir rw,relatime shared:23 - tmpfs tmpfs rw
";

    #[test]
    fn azure_files_nfs_keeps_its_locks_on_the_server() {
        let locking = classify(TABLE, Path::new("/mnt/files/holos/store"));
        assert!(
            matches!(&locking, Locking::Shared { fstype, .. } if fstype == "nfs4"),
            "{locking:?}"
        );
        assert!(locking.is_network());
    }

    #[test]
    fn blob_nfs_keeps_them_on_the_client() {
        let locking = classify(TABLE, Path::new("/mnt/blob/store"));
        assert_eq!(
            locking,
            Locking::LocalOnly {
                fstype: "nfs".to_owned(),
                mount_point: PathBuf::from("/mnt/blob"),
                option: "nolock".to_owned(),
            }
        );
    }

    #[test]
    fn netapp_files_over_nfs3_with_nlm_is_shared() {
        assert!(matches!(
            classify(TABLE, Path::new("/mnt/anf/store")),
            Locking::Shared { .. }
        ));
    }

    #[test]
    fn the_deepest_mount_decides_and_escapes_are_read() {
        let locking = classify(TABLE, Path::new("/mnt/files/inner dir/store"));
        assert!(
            matches!(&locking, Locking::Shared { fstype, .. } if fstype == "tmpfs"),
            "{locking:?}"
        );
        assert!(!locking.is_network());
        let root = classify(TABLE, Path::new("/var/lib/holos"));
        assert!(matches!(&root, Locking::Shared { fstype, .. } if fstype == "ext4"));
        assert_eq!(classify("", Path::new("/x")), Locking::Unknown);
    }

    #[test]
    fn a_sibling_with_a_common_prefix_is_not_inside() {
        // `/mnt/blobstore` starts with the characters of `/mnt/blob` but is not under it.
        let locking = classify(TABLE, Path::new("/mnt/blobstore/store"));
        assert!(matches!(&locking, Locking::Shared { fstype, .. } if fstype == "ext4"));
    }
}
