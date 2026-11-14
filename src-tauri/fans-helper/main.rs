//! desk-fans: run as root through the sudoers rule desk installs
//! (/etc/sudoers.d/desk-fans), which allows exactly `max` and `auto`.
//! `max` puts every fan at full speed, `auto` gives them back to macOS,
//! `version` prints the protocol desk checks (no root needed). Nothing else is
//! accepted, so the rule cannot be used to write any other SMC key.

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
        _ => {
            eprintln!("usage: desk-fans max|auto|version");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(target_os = "macos")]
fn run(cmd: &str) -> i32 {
    let result = smc::IoSmc::open().and_then(|mut smc| {
        if cmd == "max" {
            let mut pause = || std::thread::sleep(std::time::Duration::from_millis(50));
            fan_control::force_max(&mut smc, &mut pause)
        } else {
            fan_control::restore_auto(&mut smc)
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

#[cfg(not(target_os = "macos"))]
fn run(_cmd: &str) -> i32 {
    eprintln!("unsupported");
    2
}
