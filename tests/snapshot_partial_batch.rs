// A write batch takes its seqno, inserts its items, then publishes the seqno
// as visible. A flush or compaction finishing in between must not publish a
// higher seqno, or a snapshot at the visible seqno would see the batch partially.

use lsm_tree::{get_tmp_folder, AbstractTree, Config, SequenceNumberCounter};
use test_log::test;

#[test]
fn snapshot_partial_batch_flush() -> lsm_tree::Result<()> {
    let folder = get_tmp_folder();

    let seqno = SequenceNumberCounter::default();
    let visible_seqno = SequenceNumberCounter::default();

    let tree = Config::new(&folder, seqno.clone(), visible_seqno.clone()).open()?;

    let batch_seqno = seqno.next();
    tree.insert("a", "a", batch_seqno);

    // Flush finishes while the batch is still being written
    tree.flush_active_memtable(0)?;
    assert!(!tree.contains_key("a", visible_seqno.get())?);

    tree.insert("b", "b", batch_seqno);
    assert!(!tree.contains_key("a", visible_seqno.get())?);
    assert!(!tree.contains_key("b", visible_seqno.get())?);

    visible_seqno.fetch_max(batch_seqno + 1);
    assert!(tree.contains_key("a", visible_seqno.get())?);
    assert!(tree.contains_key("b", visible_seqno.get())?);

    Ok(())
}

#[test]
fn snapshot_partial_batch_compaction() -> lsm_tree::Result<()> {
    let folder = get_tmp_folder();

    let seqno = SequenceNumberCounter::default();
    let visible_seqno = SequenceNumberCounter::default();

    let tree = Config::new(&folder, seqno.clone(), visible_seqno.clone()).open()?;

    for key in ["a", "b"] {
        let batch_seqno = seqno.next();
        tree.insert(key, key, batch_seqno);
        visible_seqno.fetch_max(batch_seqno + 1);
        tree.flush_active_memtable(0)?;
    }
    assert_eq!(2, tree.table_count());

    let batch_seqno = seqno.next();
    tree.insert("c", "c", batch_seqno);

    // Compaction finishes while the batch is still being written
    tree.major_compact(u64::MAX, 0)?;
    assert_eq!(1, tree.table_count());
    assert!(tree.contains_key("a", visible_seqno.get())?);
    assert!(!tree.contains_key("c", visible_seqno.get())?);

    tree.insert("d", "d", batch_seqno);
    assert!(!tree.contains_key("c", visible_seqno.get())?);
    assert!(!tree.contains_key("d", visible_seqno.get())?);

    visible_seqno.fetch_max(batch_seqno + 1);
    assert!(tree.contains_key("c", visible_seqno.get())?);
    assert!(tree.contains_key("d", visible_seqno.get())?);

    Ok(())
}
