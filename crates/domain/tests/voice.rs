use instantpad_domain::voice::{PitchShape, VoiceEffect, VoiceParam, VoiceSettings};

fn with(effect: VoiceEffect) -> VoiceSettings {
    let mut settings = VoiceSettings::default();
    settings.select(effect);
    settings
}

fn fixed_semitones(settings: &VoiceSettings) -> f32 {
    match settings.shift().unwrap().pitch {
        PitchShape::Fixed { semitones } => semitones,
        PitchShape::Cycle { .. } => panic!("expected a fixed pitch"),
    }
}

#[test]
fn the_natural_voice_is_left_untouched() {
    assert_eq!(VoiceSettings::default().shift(), None);
}

#[test]
fn a_thin_voice_goes_up_and_a_deep_one_goes_down() {
    assert!(fixed_semitones(&with(VoiceEffect::Thin)) > 0.0);
    assert!(fixed_semitones(&with(VoiceEffect::Deep)) < 0.0);
}

#[test]
fn more_intensity_moves_the_pitch_further() {
    let mut settings = with(VoiceEffect::Deep);
    settings.set(VoiceParam::DeepIntensity, 0.0);
    let soft = fixed_semitones(&settings);

    settings.set(VoiceParam::DeepIntensity, 1.0);

    assert!(fixed_semitones(&settings) < soft);
}

#[test]
fn the_pitch_cycle_follows_its_three_settings() {
    let mut settings = with(VoiceEffect::PitchCycle);
    settings.set(VoiceParam::CycleTime, 1.0);
    settings.set(VoiceParam::CycleHigh, 0.5);
    settings.set(VoiceParam::CycleLow, 1.0);

    assert_eq!(
        settings.shift().unwrap().pitch,
        PitchShape::Cycle {
            low: -12.0,
            high: 6.0,
            seconds: 6.0
        }
    );
}

#[test]
fn a_value_past_the_knob_is_limited() {
    let mut settings = VoiceSettings::default();

    settings.set(VoiceParam::ThinIntensity, 7.0);

    assert_eq!(settings.get(VoiceParam::ThinIntensity), 1.0);
}

#[test]
fn switching_off_and_on_returns_to_the_last_effect() {
    let mut settings = with(VoiceEffect::Deep);

    settings.toggle();
    assert_eq!(settings.effect, VoiceEffect::Natural);

    settings.toggle();
    assert_eq!(settings.effect, VoiceEffect::Deep);
}

#[test]
fn the_next_effect_wraps_around_and_skips_the_natural_voice() {
    let mut settings = VoiceSettings::default();

    settings.select_next();
    assert_eq!(settings.effect, VoiceEffect::Thin);

    settings.select(VoiceEffect::PitchCycle);
    settings.select_next();
    assert_eq!(settings.effect, VoiceEffect::Thin);
}

#[test]
fn values_read_in_the_units_of_the_effect() {
    let mut settings = VoiceSettings::default();
    settings.set(VoiceParam::CycleTime, 1.0);
    settings.set(VoiceParam::CycleHigh, 0.5);
    settings.set(VoiceParam::CycleLow, 1.0);

    assert_eq!(settings.describe(VoiceParam::CycleTime), "6.0 s");
    assert_eq!(settings.describe(VoiceParam::CycleHigh), "+6.0 st");
    assert_eq!(settings.describe(VoiceParam::CycleLow), "-12.0 st");
}
