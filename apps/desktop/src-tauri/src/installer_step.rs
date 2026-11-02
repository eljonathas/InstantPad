//! What the Windows installer asks of the app before it first opens: whether the virtual cable is
//! there, and to install it. The app then starts with the cable already in place.

/// The exit codes the installer reads.
const DONE: i32 = 0;
const NOT_DONE: i32 = 1;

/// Runs the step named after the program in `args`, or returns `None` to start the app.
pub fn run(mut args: impl Iterator<Item = String>) -> Option<i32> {
    match args.nth(1)?.as_str() {
        "--check-cable" => Some(check_cable()),
        "--install-cable" => Some(install_cable()),
        _ => None,
    }
}

fn check_cable() -> i32 {
    let outputs = instantpad_audio::outputs();
    let cable = instantpad_platform::virtual_cable();
    if cable.find_game_output(&outputs).is_some() {
        DONE
    } else {
        NOT_DONE
    }
}

/// Ends when the driver's own setup closes, so the installer can offer the restart it needs.
fn install_cable() -> i32 {
    let cable = instantpad_platform::virtual_cable();
    match tauri::async_runtime::block_on(cable.install(&|_| {})) {
        Ok(()) => DONE,
        Err(_) => NOT_DONE,
    }
}

#[cfg(test)]
mod tests {
    use super::run;

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter()
            .map(|arg| (*arg).to_owned())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn with_no_step_or_an_unknown_one_the_app_starts() {
        assert_eq!(run(args(&["InstantPad.exe"])), None);
        assert_eq!(run(args(&["InstantPad.exe", "--minimized"])), None);
    }
}
