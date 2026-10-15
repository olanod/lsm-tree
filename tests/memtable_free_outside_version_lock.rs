// A version popped from the version history must not be dropped under the
// version history's write lock: it may hold the last reference to a flushed
// memtable, and freeing a large memtable there blocks every reader of the tree.

use lsm_tree::{
    get_tmp_folder, AbstractTree, Config, InternalValue, Memtable, SeqNo, SequenceNumberCounter,
    ValueType,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use test_log::test;

/// Items in the memtable to start with: doubled until freeing them takes
/// 50 ms or more, up to `MAX_ITEMS`.
const ITEMS: u64 = 1_500_000;
const MAX_ITEMS: u64 = 6_000_000;

fn key(item: u64) -> [u8; 8] {
    item.to_be_bytes()
}

fn filled_memtable(items: u64) -> Memtable {
    let memtable = Memtable::new(0);
    for item in 0..items {
        memtable.insert(InternalValue::from_components(
            key(item),
            key(item),
            item,
            ValueType::Value,
        ));
    }
    memtable
}

#[test]
fn memtable_free_outside_version_lock() -> lsm_tree::Result<()> {
    // Find a memtable size whose free takes long enough to tell from noise
    let mut items = ITEMS;
    let free_duration = loop {
        let memtable = filled_memtable(items);
        let start = Instant::now();
        drop(memtable);
        let free_duration = start.elapsed();

        if free_duration >= Duration::from_millis(50) {
            break free_duration;
        }
        if items >= MAX_ITEMS {
            eprintln!("skipped: freeing {items} items only takes {free_duration:?}");
            return Ok(());
        }
        items *= 2;
    };

    let folder = get_tmp_folder();
    let seqno = SequenceNumberCounter::default();
    let tree =
        Arc::new(Config::new(&folder, seqno.clone(), SequenceNumberCounter::default()).open()?);

    for item in 0..items {
        tree.insert(key(item), key(item), seqno.next());
    }
    let memtable = Arc::downgrade(&tree.active_memtable());

    // With a GC watermark of 0, the old versions (and the memtable) are kept
    tree.flush_active_memtable(0)?;
    assert!(memtable.strong_count() > 0);

    // The next flush pops them, dropping the last reference to the memtable,
    // while another thread keeps reading
    tree.insert(key(items), key(items), seqno.next());

    let reading = Arc::new(AtomicBool::new(true));
    let reader = {
        let tree = tree.clone();
        let reading = reading.clone();

        std::thread::spawn(move || {
            let mut longest_read = Duration::ZERO;
            let mut reads = 0u64;

            while reading.load(Ordering::Relaxed) {
                let start = Instant::now();
                let value = tree
                    .get(key(reads % items), SeqNo::MAX)
                    .expect("should read");
                longest_read = longest_read.max(start.elapsed());
                assert!(value.is_some());
                reads += 1;
            }

            (longest_read, reads)
        })
    };

    std::thread::sleep(Duration::from_millis(20));
    tree.flush_active_memtable(SeqNo::MAX)?;

    let deadline = Instant::now() + Duration::from_secs(30);
    while memtable.strong_count() > 0 {
        assert!(Instant::now() < deadline, "memtable was never freed");
        std::thread::sleep(Duration::from_millis(1));
    }

    // The strong count drops before the free runs, so keep reading through it
    std::thread::sleep(free_duration);
    reading.store(false, Ordering::Relaxed);
    let (longest_read, reads) = reader.join().expect("reader should not panic");

    assert!(reads > 0);
    assert!(
        longest_read < free_duration / 2,
        "a read took {longest_read:?} while freeing a memtable that takes {free_duration:?}",
    );

    Ok(())
}
