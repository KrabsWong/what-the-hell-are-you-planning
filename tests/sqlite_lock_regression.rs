//! Dependency regression for the bundled SQLite version and the Unix WAL
//! open/close lock path. This targets the SQLite 3.51.2/3.51.3 fixes; it does
//! not serialize the UI and does not touch application state.

use rusqlite::Connection;
use std::sync::mpsc;
use std::time::Duration;

fn version_tuple() -> Vec<u32> {
    let connection = Connection::open_in_memory().expect("open in-memory sqlite");
    let version: String = connection
        .query_row("select sqlite_version()", [], |row| row.get(0))
        .expect("read sqlite_version()");
    println!("bundled sqlite version: {version}");
    version
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

#[test]
fn bundled_sqlite_includes_unix_lock_and_wal_fixes() {
    let version = version_tuple();
    assert!(
        version >= vec![3, 51, 3],
        "bundled SQLite must be >= 3.51.3, got {version:?}"
    );
}

#[test]
fn concurrent_wal_open_close_completes_without_lock_stall() {
    let directory = std::env::temp_dir().join(format!(
        "sqlite-wal-lock-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).expect("create temp directory");
    let path = directory.join("lock.sqlite3");

    {
        let connection = Connection::open(&path).expect("create database");
        connection
            .execute_batch(
                "pragma journal_mode=wal;
                 pragma synchronous=normal;
                 create table sample(id integer primary key, value text not null);
                 insert into sample(value) values ('seed');",
            )
            .expect("initialize wal database");
    }

    const WORKERS: usize = 8;
    const ITERATIONS: usize = 80;
    let (done_tx, done_rx) = mpsc::channel();
    for worker in 0..WORKERS {
        let path = path.clone();
        let done_tx = done_tx.clone();
        std::thread::spawn(move || {
            for iteration in 0..ITERATIONS {
                let connection = Connection::open(&path).expect("open concurrent sqlite");
                connection
                    .busy_timeout(Duration::from_millis(500))
                    .expect("set busy timeout");
                if (worker + iteration) % 3 == 0 {
                    connection
                        .execute_batch(&format!(
                            "begin immediate;
                             insert into sample(value) values ('w{worker}-i{iteration}');
                             commit;"
                        ))
                        .expect("concurrent wal write");
                } else {
                    let _: i64 = connection
                        .query_row("select count(*) from sample", [], |row| row.get(0))
                        .expect("concurrent wal read");
                }
                if iteration % 10 == 0 {
                    let _: (i64, i64, i64) = connection
                        .query_row("pragma wal_checkpoint(passive)", [], |row| {
                            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                        })
                        .expect("wal checkpoint");
                }
                drop(connection);
            }
            let _ = done_tx.send(worker);
        });
    }
    drop(done_tx);

    for _ in 0..WORKERS {
        done_rx
            .recv_timeout(Duration::from_secs(20))
            .expect("concurrent WAL open/close must not stall on a SQLite lock");
    }

    // Exiting threads is not enough: verify the committed writes are intact and
    // that every expected write landed exactly once.
    let expected_writes = (0..WORKERS)
        .flat_map(|worker| (0..ITERATIONS).map(move |iteration| (worker, iteration)))
        .filter(|(worker, iteration)| (worker + iteration) % 3 == 0)
        .count();
    let expected_rows = 1 + expected_writes as i64;
    let connection = Connection::open(&path).expect("open database for verification");
    let integrity: String = connection
        .query_row("pragma integrity_check", [], |row| row.get(0))
        .expect("integrity check");
    assert_eq!(integrity, "ok", "database integrity after concurrent writers");
    let rows: i64 = connection
        .query_row("select count(*) from sample", [], |row| row.get(0))
        .expect("count committed rows");
    assert_eq!(
        rows, expected_rows,
        "every concurrent committed write must be present exactly once"
    );
    drop(connection);
    let _ = std::fs::remove_dir_all(directory);
}
