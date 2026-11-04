//! When to open again a device whose stream stopped by itself: a headset that reconnects, a
//! format changed in the system settings, a virtual cable being reconfigured. The app checks
//! the devices once a tick, so every wait here is counted in ticks.

use crate::ports::{DeviceRole, StreamStop};

/// Losses and failed attempts in a row before the app stops trying and leaves it to the user.
const MAX_FAILURES: u32 = 8;
/// The longest wait between two attempts.
const MAX_WAIT: u32 = 16;
/// How long a role must run before its failures are forgiven. A device that opens and then fails
/// every time still runs out of attempts, instead of being opened again forever.
const STEADY_TICKS: u32 = 60;
/// The wait between two looks for the chosen device while another one stands in for it.
const LOOK_TICKS: u32 = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Running,
    /// Opens the role again once `wait` runs out.
    Reopening { wait: u32 },
    /// Runs on another device while the chosen one is missing, and looks for it once `wait`
    /// runs out.
    StandingIn { wait: u32 },
    /// Left to the user, who brings it back by choosing a device or loading the devices again.
    Stopped,
}

#[derive(Clone, Copy, Debug, Default)]
struct RoleRecovery {
    state: State,
    failures: u32,
    /// Ticks since the role last opened or stopped.
    steady: u32,
}

impl RoleRecovery {
    /// Counts a failure and waits longer each time. Returns true when it gave up.
    fn fail(&mut self) -> bool {
        self.failures += 1;
        self.steady = 0;
        if self.failures > MAX_FAILURES {
            self.state = State::Stopped;
            return true;
        }
        // The first attempt is at once, which is all a reconfigured device needs.
        let wait = match self.failures {
            0 | 1 => 0,
            n => (1 << (n - 2).min(16)).min(MAX_WAIT),
        };
        self.state = State::Reopening { wait };
        false
    }
}

/// What to tell the user about a stream that stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Notice {
    Reconnecting,
    GaveUp,
    Failed,
}

/// What a role needs on this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Wait,
    /// Open the role again, on the chosen device or one in its place.
    Reopen,
    /// Open the role again only if the chosen device is back.
    LookForChosen,
}

#[derive(Debug, Default)]
pub(crate) struct Recovery {
    roles: [RoleRecovery; 3],
}

impl Recovery {
    fn role(&mut self, role: DeviceRole) -> &mut RoleRecovery {
        &mut self.roles[role as usize]
    }

    /// A stream stopped by itself. A role already being brought back, or left to the user,
    /// has nothing new to say.
    pub(crate) fn stopped(&mut self, role: DeviceRole, stop: StreamStop) -> Option<Notice> {
        let recovery = self.role(role);
        if matches!(recovery.state, State::Reopening { .. } | State::Stopped) {
            return None;
        }
        match stop {
            StreamStop::Failed => {
                recovery.state = State::Stopped;
                Some(Notice::Failed)
            }
            StreamStop::Lost if recovery.fail() => Some(Notice::GaveUp),
            StreamStop::Lost => Some(Notice::Reconnecting),
        }
    }

    /// Moves the role on by a tick and says what it needs now.
    pub(crate) fn tick(&mut self, role: DeviceRole) -> Step {
        let recovery = self.role(role);
        if matches!(recovery.state, State::Running | State::StandingIn { .. }) {
            recovery.steady = recovery.steady.saturating_add(1);
            if recovery.steady >= STEADY_TICKS {
                recovery.failures = 0;
            }
        }
        match &mut recovery.state {
            State::Reopening { wait: 0 } => Step::Reopen,
            State::StandingIn { wait: 0 } => {
                recovery.state = State::StandingIn { wait: LOOK_TICKS };
                Step::LookForChosen
            }
            State::Reopening { wait } | State::StandingIn { wait } => {
                *wait -= 1;
                Step::Wait
            }
            State::Running | State::Stopped => Step::Wait,
        }
    }

    /// The role opened again, on the chosen device or on one standing in for it.
    pub(crate) fn reopened(&mut self, role: DeviceRole, is_stand_in: bool) {
        let recovery = self.role(role);
        recovery.steady = 0;
        recovery.state = if is_stand_in {
            State::StandingIn { wait: LOOK_TICKS }
        } else {
            State::Running
        };
    }

    /// Opening the role again failed, or there was no device to open. Returns true when it gave
    /// up.
    pub(crate) fn failed(&mut self, role: DeviceRole) -> bool {
        self.role(role).fail()
    }

    /// The user chose a device for the role, which starts it over.
    pub(crate) fn chosen(&mut self, role: DeviceRole) {
        *self.role(role) = RoleRecovery::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIC: DeviceRole = DeviceRole::Microphone;

    /// Ticks until the role needs something, and how many it took.
    fn next_step(recovery: &mut Recovery) -> (u32, Step) {
        for ticks in 1..1000 {
            let step = recovery.tick(MIC);
            if step != Step::Wait {
                return (ticks, step);
            }
        }
        panic!("the role never needed anything");
    }

    #[test]
    fn a_lost_device_is_opened_again_on_the_first_tick() {
        let mut recovery = Recovery::default();

        assert_eq!(
            recovery.stopped(MIC, StreamStop::Lost),
            Some(Notice::Reconnecting)
        );

        assert_eq!(next_step(&mut recovery), (1, Step::Reopen));
    }

    #[test]
    fn each_failed_attempt_waits_longer_up_to_a_limit_and_then_gives_up() {
        let mut recovery = Recovery::default();
        recovery.stopped(MIC, StreamStop::Lost);
        let mut waits = Vec::new();

        let mut gave_up = false;
        while !gave_up {
            let (ticks, step) = next_step(&mut recovery);
            assert_eq!(step, Step::Reopen);
            waits.push(ticks);
            gave_up = recovery.failed(MIC);
        }

        assert_eq!(waits, [1, 2, 3, 5, 9, 17, 17, 17]);
        assert_eq!(recovery.tick(MIC), Step::Wait);
        assert_eq!(recovery.stopped(MIC, StreamStop::Lost), None);
    }

    #[test]
    fn a_device_that_fails_right_after_opening_runs_out_of_attempts() {
        let mut recovery = Recovery::default();
        let mut reopened = 0;

        while recovery.stopped(MIC, StreamStop::Lost) == Some(Notice::Reconnecting) {
            assert_eq!(next_step(&mut recovery).1, Step::Reopen);
            recovery.reopened(MIC, false);
            reopened += 1;
        }

        assert_eq!(reopened, MAX_FAILURES);
        assert_eq!(recovery.tick(MIC), Step::Wait);
    }

    #[test]
    fn a_device_that_runs_for_a_while_is_forgiven() {
        let mut recovery = Recovery::default();

        for _ in 0..MAX_FAILURES * 2 {
            assert_eq!(
                recovery.stopped(MIC, StreamStop::Lost),
                Some(Notice::Reconnecting)
            );
            next_step(&mut recovery);
            recovery.reopened(MIC, false);
            for _ in 0..STEADY_TICKS {
                recovery.tick(MIC);
            }
        }
    }

    #[test]
    fn a_stand_in_looks_for_the_chosen_device_now_and_then() {
        let mut recovery = Recovery::default();
        recovery.stopped(MIC, StreamStop::Lost);
        next_step(&mut recovery);

        recovery.reopened(MIC, true);

        assert_eq!(next_step(&mut recovery), (5, Step::LookForChosen));
        assert_eq!(next_step(&mut recovery), (5, Step::LookForChosen));
    }

    #[test]
    fn a_stream_that_failed_for_another_reason_is_left_to_the_user() {
        let mut recovery = Recovery::default();

        assert_eq!(
            recovery.stopped(MIC, StreamStop::Failed),
            Some(Notice::Failed)
        );
        assert_eq!(recovery.tick(MIC), Step::Wait);

        recovery.chosen(MIC);
        assert_eq!(
            recovery.stopped(MIC, StreamStop::Lost),
            Some(Notice::Reconnecting)
        );
    }

    #[test]
    fn the_roles_recover_apart() {
        let mut recovery = Recovery::default();

        recovery.stopped(DeviceRole::GameOutput, StreamStop::Lost);

        assert_eq!(recovery.tick(MIC), Step::Wait);
        assert_eq!(recovery.tick(DeviceRole::GameOutput), Step::Reopen);
    }
}
