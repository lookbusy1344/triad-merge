//! A bare file name writes into the working directory. This test changes the process's working
//! directory; nextest runs each test in its own process.

use std::fs;
use std::path::Path;

use merge_session::io::write_atomic;

#[test]
fn a_bare_file_name_writes_into_the_working_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::env::set_current_dir(dir.path()).expect("cd");
    write_atomic(Path::new("out.txt"), b"x\n").expect("write");
    assert_eq!(fs::read(dir.path().join("out.txt")).expect("read"), b"x\n");
    let names: Vec<_> = fs::read_dir(dir.path())
        .expect("dir lists")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(names, ["out.txt"]);
}
