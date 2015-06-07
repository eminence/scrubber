#![feature(path_ext)] 
#![feature(fs_time)]

extern crate time;

use std::env::{home_dir};
use std::path::{Path, PathBuf};
use std::fs::{read_dir, remove_dir_all};
use std::fs::PathExt;
use time::{Duration, now_utc, at_utc, Timespec};



fn can_be_removed<P: AsRef<Path>>(dir: P) -> bool {
    let old = Duration::weeks(3);
    let now = now_utc();

    let mut remove = true;
    for entry in read_dir(dir).unwrap() {
        let entry = entry.unwrap().path();
        if entry.is_dir() {
            remove = remove || can_be_removed(entry);
        } else {
            let md = entry.metadata().unwrap();
            let mda = at_utc(Timespec::new(md.accessed() as i64/1000, 0));
            let mdm = at_utc(Timespec::new(md.modified() as i64/1000, 0));

            if (now - mda < old) || (now - mdm < old) {
                remove = false;
            }
        }
    }

    remove
}

fn main() {
    let mut mytmp: PathBuf = home_dir().expect("Unable to determine HOME directory");
    mytmp.push("tmp");


    for entry in read_dir(mytmp).unwrap() {
        let entry = entry.unwrap().path();
        if can_be_removed(&entry) {
            println!("{} can be removed", entry.display());
            remove_dir_all(entry);
        } else {
            println!("must save {}", entry.display());
        }

    }



}
