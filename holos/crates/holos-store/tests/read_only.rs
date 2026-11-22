//! A store opened read-only: shared by several readers at once, written by none.
//!
//! This is what lets a checkpoint on a shared volume (NFS or SMB, on Azure or anywhere)
//! serve several read replicas: RocksDB's read-only mode takes no lock and the store writes
//! nothing to the directory, so the replicas neither exclude nor disturb one another.

#![cfg(feature = "rocksdb")]

use holos_store::{RocksStorage, Store};
use oxrdf::{GraphName, NamedNode, Quad, Term};

fn quad(n: usize) -> Quad {
    let ex = |s: String| NamedNode::new_unchecked(format!("http://example.org/{s}"));
    Quad {
        subject: ex(format!("s{n}")).into(),
        predicate: ex("p".to_owned()),
        object: Term::NamedNode(ex(format!("o{n}"))),
        graph_name: GraphName::DefaultGraph,
    }
}

#[test]
fn several_readers_share_a_checkpoint_and_none_can_write() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let live = scratch.path().join("live");
    let snapshot = scratch.path().join("snap");
    {
        let mut store = Store::with_storage(RocksStorage::open(&live).expect("open"));
        for n in 0..50 {
            store.insert(quad(n).as_ref()).expect("insert");
        }
        store.checkpoint(&snapshot).expect("checkpoint");
    }
    let files_before: Vec<_> = std::fs::read_dir(&snapshot)
        .expect("listing")
        .map(|e| e.expect("entry").file_name())
        .collect();

    // Two at once: a writable open of the same directory would be refused by the lock.
    let mut first = Store::with_storage(RocksStorage::open_read_only(&snapshot).expect("first"));
    let second = Store::with_storage(RocksStorage::open_read_only(&snapshot).expect("second"));
    for store in [&first, &second] {
        assert_eq!(store.len(), 50);
        assert!(store.contains(quad(7).as_ref()).expect("contains"));
        assert_eq!(store.iter().filter(Result::is_ok).count(), 50);
    }
    assert!(
        first.insert(quad(99).as_ref()).is_err(),
        "a read-only store took a write"
    );

    let mut files_after: Vec<_> = std::fs::read_dir(&snapshot)
        .expect("listing")
        .map(|e| e.expect("entry").file_name())
        .collect();
    let mut before = files_before;
    before.sort();
    files_after.sort();
    assert_eq!(
        before, files_after,
        "opening read-only wrote to the store directory"
    );
}

#[test]
fn a_directory_that_is_not_a_store_is_refused_read_only() {
    let scratch = tempfile::tempdir().expect("temp dir");
    assert!(RocksStorage::open_read_only(scratch.path().join("absent")).is_err());
}
