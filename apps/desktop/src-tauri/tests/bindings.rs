use std::path::PathBuf;
use std::{env, fs};

use instantpad_desktop::{BINDINGS_FILE, export_bindings};

/// Fails when the committed bindings drifted from the commands and events.
/// Run with `UPDATE_BINDINGS=1` to rewrite them.
#[test]
fn the_typescript_bindings_are_current() {
    let committed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../src/shared/ipc")
        .join(BINDINGS_FILE);

    if env::var_os("UPDATE_BINDINGS").is_some() {
        export_bindings(&committed);
        return;
    }

    let generated =
        env::temp_dir().join(format!("instantpad-{}-{BINDINGS_FILE}", std::process::id()));
    export_bindings(&generated);
    let current = fs::read_to_string(&generated).expect("read the generated bindings");
    fs::remove_file(&generated).expect("remove the generated bindings");

    assert_eq!(
        fs::read_to_string(&committed).expect("read the committed bindings"),
        current,
        "run UPDATE_BINDINGS=1 cargo test -p instantpad-desktop --test bindings",
    );
}
