mod support;

use instantpad_domain::sounds::SoundSource;
use support::World;

fn online(name: &str) -> SoundSource {
    SoundSource {
        name: name.to_owned(),
        color: "#ffffff".to_owned(),
        path: format!("store/{name}.mp3"),
        sound_url: Some(format!("https://site/{name}.mp3")),
    }
}

#[tokio::test]
async fn favoriting_downloads_the_sound_so_it_survives_leaving_the_site() {
    let world = World::new();

    assert!(world.favorites.toggle(&online("a")).await.unwrap());

    assert_eq!(world.catalog.downloaded.lock().unwrap().len(), 1);
    assert!(world.favorites.is_favorite("store/a.mp3"));
}

#[tokio::test]
async fn favoriting_again_removes_it() {
    let world = World::new();
    world.favorites.toggle(&online("a")).await.unwrap();

    assert!(!world.favorites.toggle(&online("a")).await.unwrap());

    assert!(world.favorites.list().is_empty());
}

#[tokio::test]
async fn a_local_file_that_is_gone_cannot_become_a_favorite() {
    let world = World::new();
    let local = SoundSource {
        sound_url: None,
        ..online("a")
    };

    assert!(world.favorites.toggle(&local).await.is_err());

    assert!(world.favorites.list().is_empty());
}
