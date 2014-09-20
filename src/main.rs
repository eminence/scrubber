#![feature(phase)]

extern crate time;
extern crate serialize;
#[phase(plugin)] extern crate docopt_macros;
extern crate docopt;

#[phase(plugin)]
extern crate lazy_static;


use std::collections::HashMap;
use std::io::fs::PathExtensions;
use std::io::fs;
use time::{get_time};
use docopt::FlagParser;
use std::char::is_alphabetic;
use std::from_str::{FromStr, from_str};


docopt!(Args, "
Usage:
  scrubber -h | --help
  scrubber [options] <directory>

Options:
    -h --help      Show this help
    -n --dry-run   Don't actually delete anything
    --age=<age>    What is considered old enough to delete.  Default is 30 days
    -v --verbose   Be verbose    
")

fn get_now() -> u64 {
    let now = get_time();
    now.sec as u64
}

#[deriving(Eq,PartialEq,Show,Hash)]
enum AgeUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
    Months,
    Years
}

lazy_static! {
    static ref AgeUnitMap: HashMap<AgeUnit, u64> = {
        let mut m = HashMap::new();
        m.insert(Seconds, 1);
        m.insert(Minutes, 60);
        m.insert(Hours, 60*60);
        m.insert(Days, 60*60*24);
        m.insert(Months, 60*60*24*30);
        m.insert(Years, 60*60*24*365);
        m
    };
}

impl FromStr for AgeUnit {
    fn from_str(s: &str) -> Option<AgeUnit> {
        if s.starts_with("sec") || s == "s" { return Some(Seconds); }
        if s.starts_with("min") || s == "m" { return Some(Minutes); }
        if s.starts_with("hour") || s.starts_with("hr") || s == "h" { return Some(Hours); }
        if s.starts_with("day") || s == "d" { return Some(Days); }
        if s.starts_with("mon") { return Some(Months); }
        if s.starts_with("year") || s.starts_with("yr") || s == "y" { return Some(Years); }

        None
    }
}

#[test]
fn test_age_unit_parsing() {
    assert_eq!(Seconds, from_str("seconds").unwrap());
    assert_eq!(Seconds, from_str("s").unwrap());
    assert_eq!(Minutes, from_str("minutes").unwrap());
    assert_eq!(Minutes, from_str("m").unwrap());
    assert_eq!(Hours, from_str("hrs").unwrap());
    assert_eq!(Days, from_str("d").unwrap());
    assert_eq!(Days, from_str("day").unwrap());
    assert_eq!(Months, from_str("mon").unwrap());
    assert_eq!(Years, from_str("yrs").unwrap());
}

fn parse_duration(s: &str) -> u64 {
    let unit_pos = s.chars().position(is_alphabetic);
    let unit = match unit_pos { 
        None => Days,
        Some(pos) => from_str(s.slice_from(pos)).unwrap()
    };

    let qty :u64 = match unit_pos {
        None => 30,
        Some(pos) => from_str(s.slice_to(pos)).unwrap()
    };

    AgeUnitMap.get(&unit) * qty
}

#[test]
fn test_parse_duration() {
    assert_eq!(parse_duration("1s"), 1);
    assert_eq!(parse_duration("2mins"), 120);
    assert_eq!(parse_duration("3hrs"), 10800);
    assert_eq!(parse_duration("4days"), 345600);
    assert_eq!(parse_duration("5months"), 2592000 * 5);
    assert_eq!(parse_duration("6yrs"), 31536000 * 6);

}

struct Config {
    verify: bool, // if set, don't actually delete anything
                now: u64, // the current timestamp, in seconds
                oldage: u64, // what age (in seconds) is considered old enough to delete
                verbose: bool
}

// returns True if this file can be deleted (But it won't actually delete it)
fn check_file(dir: &Path, cfg: &Config) -> bool {
    assert!(dir.is_file());
    let stat = match dir.stat() {
        Err(e) => fail!("Failed to stat {}: {}", dir.display(), e),
        Ok(o) => o
    };
    cfg.now - (stat.modified / 1000) > cfg.oldage && cfg.now - (stat.accessed / 1000) > cfg.oldage
}

fn rm(p: &Path, verify: bool) {
    if p.is_dir() {
        if verify { println!("Will delete dir {}", p.display()); }
        else {
            if fs::rmdir(p).is_err() { println!("Warning: Failed to rmdir {}", p.display()); }
        }
    } else if p.is_file() {
        if verify { println!("Will delete file {}", p.display()); } 
        else { if fs::unlink(p).is_err() { println!("Warning: Failed to unlink {}", p.display()); }
        }
    }
}


// Figure out if we can delete `dir`
// If we can delete all the contents of `dir` then we can delete `dir`
// returns true if we can delete
fn scan_dir(dir: &Path, cfg: &Config) -> bool {
    let mut can_delete = true;
    if cfg.verbose {
        println!("Scanning {}", dir.display());
    }

    let mut contents = match fs::readdir(dir) {
        Err(e) => fail!("Failed to read {}: {}", dir.display(), e),
        Ok(o) => o
    };
    for entry in contents.iter() {
        if entry.is_file() {
            can_delete = can_delete && check_file(entry, cfg);
        } else if entry.is_dir() {
            let del_subdir = scan_dir(entry, cfg);
            can_delete = del_subdir && can_delete;
            if del_subdir {
                rm(entry, cfg.verify);
            }
        }
    }

    // if we can delete this current directory, then do so, by unlinking every file element, 
    // and rmdir'ing every directdory
    if can_delete {
        // the contents of this directory might have changed, let's get a new listing
        contents = match fs::readdir(dir) {
            Err(e) => fail!("Failed to read {}: {}", dir.display(), e),
            Ok(o) => o
        };
        for entry in contents.iter() {
            rm(entry, cfg.verify);
        }
    }

    return can_delete;
}


fn main() {
    let args: Args = FlagParser::parse().unwrap_or_else(|e| e.exit());

    let tmpdir = Path::new(args.arg_directory);
    if !tmpdir.exists() { fail!("{} does not exist!", tmpdir.display()); }
    if !tmpdir.is_dir() { fail!("{} is not a directory!", tmpdir.display()); }

    let oldage = parse_duration(args.flag_age.as_slice());
    let cfg = Config{verbose: args.flag_verbose, verify: args.flag_dry_run, now: get_now(), oldage:oldage};
    if cfg.verbose {
        println!("Deleting anything in {} that is older than {} seconds", tmpdir.display(), oldage);
    }


    let contents = match fs::readdir(&tmpdir) {
        Err(e) => fail!("Failed to read {}: {}", tmpdir.display(), e),
        Ok(o) => o
    };
    for entry in contents.iter() {
        if entry.is_dir() {
            if scan_dir(entry, &cfg) {
                rm(entry, cfg.verify);
            }
        }
        if entry.is_file() {
            if check_file(entry, &cfg) {
                rm(entry, cfg.verify);
            }
        }
    }
}
