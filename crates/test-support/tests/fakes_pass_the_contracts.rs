use instantpad_domain::hotkeys::{Hotkey, HotkeyModifiers};
use instantpad_test_support::contract;
use instantpad_test_support::fakes::{
    FakeCable, FakeHotkeyHost, FakePlayer, InMemoryBoard, InMemoryFavorites, InMemorySettings,
    device,
};

#[tokio::test]
async fn the_fake_player() {
    contract::sound_player(&FakePlayer::default(), "a.mp3").await;
}

#[test]
fn the_fake_hotkey_host() {
    let key = Hotkey::new("F1", HotkeyModifiers::NONE);
    contract::hotkey_host(&FakeHotkeyHost::default(), &key);
}

#[test]
fn the_in_memory_repositories() {
    contract::board_repository(&InMemoryBoard::default());
    contract::favorites_repository(&InMemoryFavorites::default());
    contract::settings_repository(&InMemorySettings::default());
}

#[test]
fn the_fake_cable() {
    contract::virtual_cable(
        &FakeCable,
        &device("in", "CABLE Input"),
        &device("out", "CABLE Output"),
        &device("real", "Headphones"),
    );
}
