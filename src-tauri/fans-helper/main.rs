//! desk-fans: run as root through the sudoers rule desk installs
//! (/etc/sudoers.d/desk-fans), which allows exactly `max` and `auto`.
//! `max` puts every fan at full speed, `auto` gives them back to macOS,
//! `version` prints the protocol desk checks and `probe` what this Mac's fans
//! read (both without root; they write nothing). Nothing else is accepted, so
//! the rule cannot be used to write any other SMC key.

// The shared files carry more than the helper uses (desk reads with them too).
#![allow(dead_code)]

#[path = "../src/fan_control.rs"]
mod fan_control;
#[cfg(target_os = "macos")]
#[path = "../src/smc.rs"]
mod smc;

fn main() {
    let code = match std::env::args().nth(1).as_deref() {
        Some("version") => {
            println!("{}", fan_control::PROTOCOL);
            0
        }
        Some(cmd @ ("max" | "auto")) => run(cmd),
        Some("probe") => probe(),
        _ => {
            eprintln!("usage: desk-fans max|auto|version|probe");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(target_os = "macos")]
fn run(cmd: &str) -> i32 {
    let mut pause = || std::thread::sleep(std::time::Duration::from_millis(100));
    let result = smc::IoSmc::open().and_then(|mut smc| {
        if cmd == "max" {
            fan_control::force_max(&mut smc, &mut pause)
        } else {
            fan_control::restore_auto(&mut smc, &mut pause)
        }
    });
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// The keys and fans as desk sees them, to send in when a Mac misbehaves.
#[cfg(target_os = "macos")]
fn probe() -> i32 {
    let fans = smc::IoSmc::open().and_then(|mut smc| {
        let keys = fan_control::Keys::probe(&mut smc);
        println!("mode key {}, Ftst {}", keys.mode(0), if keys.unlock { "yes" } else { "no" });
        fan_control::read(&mut smc)
    });
    match fans {
        Ok(fans) => {
            println!("{} fans", fans.len());
            for (i, f) in fans.iter().enumerate() {
                println!("fan {i}: mode {}, {:.0} rpm, max {:.0}", f.mode, f.rpm, f.max_rpm);
            }
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn probe() -> i32 {
    eprintln!("unsupported");
    2
}

#[cfg(not(target_os = "macos"))]
fn run(_cmd: &str) -> i32 {
    eprintln!("unsupported");
    2
}
