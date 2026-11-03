use instantpad_domain::training::{
    GraphicsCard, MAX_TAKE_SECONDS, MAX_TRAINING_SECONDS, MIN_TRAINING_SECONDS, ProcessorReason,
    System, TRAINING_WORK_MEGABYTES, TakeRefusal, TrainingDevice, TrainingJob, TrainingPlan,
    missing_megabytes, needed_megabytes,
};

fn card(memory_megabytes: u32, driver: &str) -> GraphicsCard {
    GraphicsCard {
        name: "NVIDIA GeForce RTX 3070 Ti".to_owned(),
        memory_megabytes,
        driver: driver.to_owned(),
    }
}

fn processor(reason: ProcessorReason) -> TrainingDevice {
    TrainingDevice::Processor { reason }
}

fn job() -> TrainingJob {
    let plan = TrainingPlan::new(TrainingDevice::Card, Some(&card(8192, "610.88")));
    TrainingJob::new("ana", plan)
}

#[test]
fn a_recent_nvidia_card_with_enough_memory_trains_the_voice() {
    let device = TrainingDevice::choose(Some(&card(8192, "610.88")), System::Windows);

    assert_eq!(device, TrainingDevice::Card);
}

#[test]
fn without_an_nvidia_card_the_voice_trains_on_the_processor() {
    let device = TrainingDevice::choose(None, System::Windows);

    assert_eq!(device, processor(ProcessorReason::NoCard));
}

#[test]
fn a_card_with_little_memory_leaves_the_training_to_the_processor() {
    let device = TrainingDevice::choose(Some(&card(3072, "610.88")), System::Linux);

    assert_eq!(device, processor(ProcessorReason::SmallCard));
}

#[test]
fn a_driver_too_old_for_cuda_12_leaves_the_training_to_the_processor() {
    let old = TrainingDevice::choose(Some(&card(8192, "512.95")), System::Windows);
    let unreadable = TrainingDevice::choose(Some(&card(8192, "unknown")), System::Windows);
    let oldest = TrainingDevice::choose(Some(&card(8192, "528.33")), System::Windows);

    assert_eq!(old, processor(ProcessorReason::OldDriver));
    assert_eq!(unreadable, processor(ProcessorReason::OldDriver));
    assert_eq!(oldest, TrainingDevice::Card);
}

#[test]
fn the_oldest_driver_depends_on_the_system() {
    let driver = card(8192, "526.10");

    assert_eq!(
        TrainingDevice::choose(Some(&driver), System::Linux),
        TrainingDevice::Card
    );
    assert_eq!(
        TrainingDevice::choose(Some(&driver), System::Windows),
        processor(ProcessorReason::OldDriver)
    );
}

#[test]
fn a_card_learns_more_pieces_at_once_the_more_memory_it_has() {
    let batch = |memory| TrainingPlan::new(TrainingDevice::Card, Some(&card(memory, "610.88"))).batch;

    assert_eq!(batch(4096), 2);
    assert_eq!(batch(6144), 4);
    assert_eq!(batch(8192), 6);
    assert_eq!(batch(24_576), 8);
}

#[test]
fn the_processor_trains_fewer_epochs_and_saves_after_each_one() {
    let card = TrainingPlan::new(TrainingDevice::Card, Some(&card(8192, "610.88")));
    let processor = TrainingPlan::new(processor(ProcessorReason::NoCard), None);

    assert!(processor.epochs < card.epochs);
    assert_eq!(processor.save_every, 1);
    assert!(card.save_every > 1);
}

#[test]
fn forty_minutes_take_about_an_hour_on_a_card_and_more_than_a_day_on_the_processor() {
    let card = TrainingPlan::new(TrainingDevice::Card, Some(&card(8192, "610.88")));
    let processor = TrainingPlan::new(processor(ProcessorReason::NoCard), None);

    let on_card = card.estimated_seconds(MAX_TRAINING_SECONDS);
    let on_processor = processor.estimated_seconds(MAX_TRAINING_SECONDS);

    assert!((2_400..=7_200).contains(&on_card), "{on_card} s");
    assert!(on_processor > 16 * 3_600, "{on_processor} s");
}

#[test]
fn what_is_left_comes_from_how_long_the_epochs_took() {
    let plan = TrainingPlan::new(TrainingDevice::Card, Some(&card(8192, "610.88")));

    assert_eq!(plan.remaining_seconds(150, 20.0), 1_000);
    assert_eq!(plan.remaining_seconds(plan.epochs, 20.0), 0);
    assert_eq!(plan.remaining_seconds(plan.epochs + 5, 20.0), 0);
}

#[test]
fn missing_tools_count_their_download_and_install_on_top_of_the_training() {
    assert_eq!(
        needed_megabytes(4_200, 9_000, false),
        4_200 + 9_000 + TRAINING_WORK_MEGABYTES
    );
    assert_eq!(needed_megabytes(4_200, 9_000, true), TRAINING_WORK_MEGABYTES);
}

#[test]
fn a_disk_short_of_room_says_how_much_is_missing() {
    assert_eq!(missing_megabytes(10_000, Some(4_000)), Some(6_000));
    assert_eq!(missing_megabytes(10_000, Some(10_000)), None);
    assert_eq!(
        missing_megabytes(10_000, None),
        None,
        "a disk that cannot be asked is not refused"
    );
}

#[test]
fn takes_add_up_until_the_training_has_enough() {
    let mut job = job();
    assert!(!job.has_enough());

    job.add_take(MIN_TRAINING_SECONDS - 60);
    assert!(!job.has_enough());
    job.add_take(60);

    assert!(job.has_enough());
    assert_eq!(job.seconds(), MIN_TRAINING_SECONDS);
}

#[test]
fn a_take_is_cut_at_its_longest_and_at_what_still_fits() {
    let mut job = job();
    assert_eq!(job.room_for(3_600), Ok(MAX_TAKE_SECONDS));

    for _ in 0..3 {
        job.add_take(MAX_TAKE_SECONDS);
    }
    let left = MAX_TRAINING_SECONDS - 3 * MAX_TAKE_SECONDS;
    assert_eq!(job.room_for(MAX_TAKE_SECONDS), Ok(left.min(MAX_TAKE_SECONDS)));
}

#[test]
fn a_full_training_takes_no_more_speech() {
    let mut job = job();
    for _ in 0..MAX_TRAINING_SECONDS / MAX_TAKE_SECONDS {
        job.add_take(MAX_TAKE_SECONDS);
    }

    assert_eq!(job.room_for(60), Err(TakeRefusal::Full));
}

#[test]
fn a_take_of_a_few_seconds_is_refused() {
    assert_eq!(job().room_for(2), Err(TakeRefusal::TooShort));
}

#[test]
fn takes_can_be_removed_only_before_the_training_starts() {
    let mut job = job();
    let first = job.add_take(60);
    let second = job.add_take(90);

    assert_eq!(job.remove_take(&first.id), Some(first));
    assert_eq!(job.seconds(), 90);

    job.is_started = true;
    assert_eq!(job.remove_take(&second.id), None);
    assert_eq!(job.remove_take("not a take"), None);
}

#[test]
fn every_training_and_take_has_a_folder_name_of_its_own() {
    let mut first = job();
    let second = job();
    let take = first.add_take(60);
    let other = first.add_take(60);

    assert_ne!(first.id, second.id);
    assert_ne!(take.id, other.id);
    for name in [&first.id, &take.id] {
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }
}
