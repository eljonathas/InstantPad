//! A trained model added to an AI voice, and the engine the voice speaks through.

mod support;

use std::path::PathBuf;

use instantpad_application::events::{AppEvent, EffectView, TrainedView};
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_domain::cloning::{TrainedModel, VoiceRecipe};
use instantpad_domain::voice::{
    Conversion, VoiceChain, VoiceEffect, VoiceEngine, VoiceParam, VoicePreset, VoiceTarget,
};
use support::{VoiceWorld, poll_once};

const FEMININE: &str = "feminine";

fn add(world: &VoiceWorld, effect: VoiceEffect, clone_id: Option<&str>, folder: &str) {
    poll_once(world.voice.add_trained_model(
        effect,
        clone_id.map(str::to_owned),
        PathBuf::from(folder),
    ))
    .expect("the fakes answer at once");
}

fn model_of(world: &VoiceWorld, key: &str) -> Option<TrainedModel> {
    world.library.lock().trained.get(key).cloned()
}

fn conversion(world: &VoiceWorld) -> Option<Conversion> {
    world.changer.conversion.lock().unwrap().clone()
}

fn effect<'a>(views: &'a [EffectView], effect: VoiceEffect, id: Option<&str>) -> &'a EffectView {
    views
        .iter()
        .find(|view| view.effect == effect && view.clone_id.as_deref() == id)
        .unwrap()
}

fn shown(key: TextKey) -> AppEvent {
    AppEvent::show(Message::text(key))
}

/// The feminine voice with a model from the folder "fem", speaking through it.
fn trained_feminine() -> VoiceWorld {
    let world = VoiceWorld::ready();
    add(&world, VoiceEffect::FeminineAi, None, "/kit/fem");
    world
}

#[test]
fn a_model_added_to_the_feminine_voice_is_kept_and_named_after_the_training() {
    let world = trained_feminine();

    let model = model_of(&world, FEMININE).unwrap();
    assert_eq!(model.name, "fem");
    let added = world.trained.added.lock().unwrap().clone();
    assert_eq!(added, [(PathBuf::from("/kit/fem"), model.id.clone())]);
    assert_eq!(world.repository.library.lock().unwrap().trained.len(), 1);
    assert!(world.saw(&AppEvent::show(Message::with(
        TextKey::TrainedModelAdded,
        [Message::plain("fem")]
    ))));
}

#[test]
fn the_feminine_voice_then_speaks_through_the_model_with_the_voice_as_it_is() {
    let world = trained_feminine();

    world.voice.select(VoiceEffect::FeminineAi);

    let model = model_of(&world, FEMININE).unwrap();
    assert_eq!(
        conversion(&world),
        Some(Conversion::Trained {
            model,
            semitones: 0.0
        })
    );
    assert_eq!(
        *world.changer.chain.lock().unwrap(),
        Some(VoiceChain::untouched())
    );
}

#[test]
fn a_trained_voice_needs_no_learning_of_the_users_voice() {
    let world = VoiceWorld::installed();
    add(&world, VoiceEffect::FeminineAi, None, "/kit/fem");

    world.voice.select(VoiceEffect::FeminineAi);

    assert_eq!(world.voice.view().active, VoiceEffect::FeminineAi);
    assert!(matches!(
        conversion(&world),
        Some(Conversion::Trained { .. })
    ));
    assert!(!world.is_listening());
}

#[test]
fn the_pitch_knob_of_a_trained_voice_moves_it_from_the_voices_own() {
    let world = trained_feminine();
    world.voice.select(VoiceEffect::FeminineAi);

    world.voice.set_param(VoiceParam::TrainedPitch, 0.75);

    let view = world.voice.view();
    let feminine = effect(&view.effects, VoiceEffect::FeminineAi, None);
    assert_eq!(feminine.params.len(), 1);
    assert_eq!(feminine.params[0].param, VoiceParam::TrainedPitch);
    assert_eq!(feminine.params[0].text, "+6.0 st");
    assert_eq!(conversion(&world).unwrap().semitones(), Some(6.0));
}

#[test]
fn the_instant_engine_can_be_picked_back_and_forth() {
    let world = trained_feminine();
    world.voice.select(VoiceEffect::FeminineAi);

    world
        .voice
        .set_engine(VoiceEffect::FeminineAi, None, VoiceEngine::Instant);

    assert!(matches!(
        conversion(&world),
        Some(Conversion::Timbre { .. })
    ));
    let view = world.voice.view();
    let feminine = effect(&view.effects, VoiceEffect::FeminineAi, None);
    assert_eq!(
        feminine.trained.as_ref().map(|trained| trained.engine),
        Some(VoiceEngine::Instant)
    );
    assert_eq!(feminine.params[0].param, VoiceParam::FeminineAiPitch);

    world
        .voice
        .set_engine(VoiceEffect::FeminineAi, None, VoiceEngine::Trained);

    assert!(matches!(
        conversion(&world),
        Some(Conversion::Trained { .. })
    ));
}

#[test]
fn the_instant_engine_without_the_users_voice_learns_it_first() {
    let world = VoiceWorld::installed();
    add(&world, VoiceEffect::FeminineAi, None, "/kit/fem");
    world.voice.select(VoiceEffect::FeminineAi);

    world
        .voice
        .set_engine(VoiceEffect::FeminineAi, None, VoiceEngine::Instant);

    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
    assert_eq!(conversion(&world), None);
    assert!(world.is_listening());
    assert!(world.saw(&shown(TextKey::AiListening)));
}

#[test]
fn a_model_is_added_only_once_the_ai_models_are_installed() {
    let world = VoiceWorld::new();

    add(&world, VoiceEffect::FeminineAi, None, "/kit/fem");

    assert!(world.trained.added.lock().unwrap().is_empty());
    assert_eq!(model_of(&world, FEMININE), None);
    assert!(world.saw(&AppEvent::error(Message::text(TextKey::AiNotInstalled))));
}

#[test]
fn a_folder_the_kit_did_not_export_is_reported_and_nothing_changes() {
    let world = VoiceWorld::ready();
    let refused = Failure::because(TextKey::TrainedModelInvalid, "voice.json: not found");
    *world.trained.fails.lock().unwrap() = Some(refused.clone());

    add(&world, VoiceEffect::FeminineAi, None, "/home/photos");

    assert_eq!(model_of(&world, FEMININE), None);
    assert!(world.saw(&AppEvent::error(refused.0)));
}

#[test]
fn only_the_feminine_voice_and_the_cloned_ones_take_a_model() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");
    let design = world.keep_design(
        "Mix",
        VoiceRecipe {
            base_id: "a".to_owned(),
            timbre: 0.3,
            blend_id: None,
            blend: 0.0,
        },
    );

    add(&world, VoiceEffect::Cloned, Some(&design), "/kit/mix");
    add(&world, VoiceEffect::Ogre, None, "/kit/ogre");
    add(&world, VoiceEffect::Cloned, Some("gone"), "/kit/gone");

    assert!(world.trained.added.lock().unwrap().is_empty());
    let view = world.voice.view();
    assert!(effect(&view.effects, VoiceEffect::FeminineAi, None).takes_trained_model);
    assert!(effect(&view.effects, VoiceEffect::Cloned, Some("a")).takes_trained_model);
    assert!(!effect(&view.effects, VoiceEffect::Cloned, Some(&design)).takes_trained_model);
    assert!(!effect(&view.effects, VoiceEffect::Ogre, None).takes_trained_model);
}

#[test]
fn a_cloned_voice_shows_its_model_and_engine() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");

    add(&world, VoiceEffect::Cloned, Some("a"), "/kit/ana");

    let view = world.voice.view();
    assert_eq!(
        effect(&view.effects, VoiceEffect::Cloned, Some("a")).trained,
        Some(TrainedView {
            name: "ana".to_owned(),
            credit: "Recordings".to_owned(),
            engine: VoiceEngine::Trained,
        })
    );
    assert_eq!(
        effect(&view.effects, VoiceEffect::FeminineAi, None).trained,
        None
    );
}

#[test]
fn a_new_model_replaces_the_one_before_and_its_files_go() {
    let world = trained_feminine();
    let first = model_of(&world, FEMININE).unwrap();

    add(&world, VoiceEffect::FeminineAi, None, "/kit/fem2");

    assert_eq!(model_of(&world, FEMININE).unwrap().name, "fem2");
    assert_eq!(*world.trained.removed.lock().unwrap(), [first.id]);
}

#[test]
fn removing_the_model_goes_back_to_the_instant_engine_and_deletes_its_files() {
    let world = trained_feminine();
    world.voice.select(VoiceEffect::FeminineAi);
    let model = model_of(&world, FEMININE).unwrap();

    world
        .voice
        .remove_trained_model(VoiceEffect::FeminineAi, None);

    assert_eq!(model_of(&world, FEMININE), None);
    assert_eq!(*world.trained.removed.lock().unwrap(), [model.id]);
    assert!(matches!(
        conversion(&world),
        Some(Conversion::Timbre { .. })
    ));
    assert!(world.saw(&shown(TextKey::TrainedModelRemoved)));
}

#[test]
fn deleting_a_cloned_voice_deletes_its_model_and_its_engine() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");
    add(&world, VoiceEffect::Cloned, Some("a"), "/kit/ana");
    world.voice.set_engine(
        VoiceEffect::Cloned,
        Some("a".to_owned()),
        VoiceEngine::Instant,
    );
    let model = model_of(&world, "a").unwrap();

    world.voice.delete_clone("a");

    assert_eq!(model_of(&world, "a"), None);
    assert_eq!(*world.trained.removed.lock().unwrap(), [model.id]);
    // A voice cloned again under the same id would start with its own choice.
    let target = VoiceTarget::Own("a".to_owned());
    assert_eq!(
        world.settings.lock().voice.engine(&target, true),
        VoiceEngine::Trained
    );
}

#[test]
fn the_engine_of_a_voice_is_kept_with_the_settings() {
    let world = trained_feminine();

    world
        .voice
        .set_engine(VoiceEffect::FeminineAi, None, VoiceEngine::Instant);

    let target = VoiceTarget::Preset(VoicePreset::Feminine);
    let saved = world.settings.lock().voice.clone();
    assert_eq!(saved.engine(&target, true), VoiceEngine::Instant);
    assert_eq!(saved.engine(&target, false), VoiceEngine::Instant);
}
