use instantpad_application::routing::{DefaultDevices, SavedRouting, choose};
use instantpad_test_support::fakes::{FakeCable, device};

#[test]
fn a_mic_never_reads_the_cable_and_the_game_output_is_always_the_cable() {
    let microphones = [
        device("cable-out", "CABLE Output"),
        device("mic", "USB Mic"),
    ];
    let outputs = [
        device("phones", "Headphones"),
        device("cable-in", "CABLE Input"),
    ];

    let choice = choose(
        &FakeCable,
        &microphones,
        &outputs,
        SavedRouting::default(),
        DefaultDevices {
            microphone_id: Some("cable-out"),
            output_id: Some("cable-in"),
        },
    );

    assert_eq!(choice.microphone.unwrap().id, "mic");
    assert_eq!(choice.game_output.unwrap().id, "cable-in");
    assert_eq!(choice.monitor_output.unwrap().id, "phones");
}

#[test]
fn a_saved_device_wins_over_the_system_default() {
    let microphones = [device("a", "Mic A"), device("b", "Mic B")];

    let choice = choose(
        &FakeCable,
        &microphones,
        &[],
        SavedRouting {
            microphone_id: Some("b"),
            ..SavedRouting::default()
        },
        DefaultDevices {
            microphone_id: Some("a"),
            output_id: None,
        },
    );

    assert_eq!(choice.microphone.unwrap().id, "b");
}

#[test]
fn a_saved_device_that_left_falls_back_to_the_default_then_the_first() {
    let microphones = [device("a", "Mic A"), device("b", "Mic B")];
    let saved = SavedRouting {
        microphone_id: Some("gone"),
        ..SavedRouting::default()
    };
    let with_default = DefaultDevices {
        microphone_id: Some("b"),
        output_id: None,
    };

    let by_default = choose(&FakeCable, &microphones, &[], saved, with_default);
    let by_order = choose(
        &FakeCable,
        &microphones,
        &[],
        saved,
        DefaultDevices::default(),
    );

    assert_eq!(by_default.microphone.unwrap().id, "b");
    assert_eq!(by_order.microphone.unwrap().id, "a");
}

#[test]
fn a_headset_saved_as_game_output_gives_way_to_the_cable() {
    let outputs = [
        device("phones", "Headphones"),
        device("cable", "CABLE Input"),
    ];
    let saved = SavedRouting {
        game_output_id: Some("phones"),
        ..SavedRouting::default()
    };

    let choice = choose(&FakeCable, &[], &outputs, saved, DefaultDevices::default());

    assert_eq!(choice.game_output.unwrap().id, "cable");
}

#[test]
fn without_a_cable_there_is_no_game_output() {
    let outputs = [device("phones", "Headphones")];

    let choice = choose(
        &FakeCable,
        &[],
        &outputs,
        SavedRouting::default(),
        DefaultDevices::default(),
    );

    assert_eq!(choice.game_output, None);
}
