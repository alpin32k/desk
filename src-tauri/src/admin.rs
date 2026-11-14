//! A fixed script run once as root behind macOS's own password dialog, and the
//! user-name check for the sudoers rules desk installs that way (coffee.rs:
//! pmset for the lid, fans.rs: the fan helper).

/// Run a fixed shell command as root; macOS shows its own password dialog
/// with `prompt`. Cancelled: Err("anulowano" / "cancelled").
#[cfg(target_os = "macos")]
pub fn run_admin(script: &str, prompt: &str) -> Result<(), String> {
    let apple = format!(
        "do shell script {} with administrator privileges with prompt {}",
        applescript_quote(script),
        applescript_quote(prompt)
    );
    let out = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", &apple])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(if err.contains("-128") {
            tr!("anulowano", "cancelled").into()
        } else {
            err.trim().to_string()
        })
    }
}

pub fn applescript_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A user name safe to put in a sudoers rule.
pub fn check_user(user: &str) -> Result<(), String> {
    let ok = !user.is_empty()
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        && !user.starts_with('-');
    if ok {
        Ok(())
    } else {
        Err(tr!(
            format!("nieobsługiwana nazwa użytkownika: {user:?}"),
            format!("unsupported user name: {user:?}")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applescript_strings_are_escaped() {
        assert_eq!(applescript_quote(r#"a "b" \c"#), r#""a \"b\" \\c""#);
    }

    #[test]
    fn user_names_for_sudoers_rules() {
        assert!(check_user("kamil").is_ok());
        assert!(check_user("first.last_2-x").is_ok());
        for bad in ["", "a b", "x ALL=(ALL) ALL", "-x"] {
            assert!(check_user(bad).is_err(), "{bad:?}");
        }
    }
}
