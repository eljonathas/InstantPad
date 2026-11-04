//! What the kit says it is doing, one line of JSON after `@instantpad ` when it runs with
//! `--events`, and what the system says about the NVIDIA card.

use instantpad_application::ports::TrainingStage;
use instantpad_domain::training::GraphicsCard;
use serde_json::Value;

const PREFIX: &str = "@instantpad ";

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum KitEvent {
    Stage(TrainingStage),
    /// The epoch RVC starts from, 0 or its last save.
    Learning {
        epoch: u32,
    },
    Epoch {
        epoch: u32,
        seconds: f32,
    },
    Download {
        file: String,
        done: u64,
        size: u64,
    },
    Exported(String),
    Error(String),
}

/// The event on a line the kit wrote, or `None` for any other line, such as RVC's own output.
pub(crate) fn parse(line: &str) -> Option<KitEvent> {
    let json: Value = serde_json::from_str(line.trim().strip_prefix(PREFIX)?).ok()?;
    let number = |key: &str| json.get(key).and_then(Value::as_u64);
    let text = |key: &str| json.get(key).and_then(Value::as_str).map(str::to_owned);

    if let Some(message) = text("error") {
        return Some(KitEvent::Error(message));
    }
    if let Some(folder) = text("exported") {
        return Some(KitEvent::Exported(folder));
    }
    if let Some(file) = text("download") {
        return Some(KitEvent::Download {
            file,
            done: number("done")?,
            size: number("size")?,
        });
    }
    if let Some(step) = text("step") {
        return Some(match step.as_str() {
            "cut" => KitEvent::Stage(TrainingStage::Cutting),
            "pitch" => KitEvent::Stage(TrainingStage::Pitch),
            "words" => KitEvent::Stage(TrainingStage::Words),
            "learn" => KitEvent::Learning {
                epoch: u32::try_from(number("epoch").unwrap_or(0)).ok()?,
            },
            "export" => KitEvent::Stage(TrainingStage::Exporting),
            _ => return None,
        });
    }
    let epoch = u32::try_from(number("epoch")?).ok()?;
    let seconds = json.get("seconds").and_then(Value::as_f64).unwrap_or(0.0) as f32;
    Some(KitEvent::Epoch { epoch, seconds })
}

/// The NVIDIA card with the most memory, from
/// `nvidia-smi --query-gpu=name,memory.total,driver_version --format=csv,noheader,nounits`.
pub(crate) fn graphics_card(output: &str) -> Option<GraphicsCard> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(',').map(str::trim);
            let name = fields.next()?.to_owned();
            let memory_megabytes = fields.next()?.parse().ok()?;
            let driver = fields.next()?.to_owned();
            (!name.is_empty()).then_some(GraphicsCard {
                name,
                memory_megabytes,
                driver,
            })
        })
        .max_by_key(|card| card.memory_megabytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_of_the_kit_become_events() {
        assert_eq!(
            parse(r#"@instantpad {"step": "cut"}"#),
            Some(KitEvent::Stage(TrainingStage::Cutting))
        );
        assert_eq!(
            parse(r#"@instantpad {"step": "learn", "epoch": 35, "epochs": 200, "device": "cuda"}"#),
            Some(KitEvent::Learning { epoch: 35 })
        );
        assert_eq!(
            parse(r#"@instantpad {"epoch": 36, "epochs": 200, "seconds": 18.5}"#),
            Some(KitEvent::Epoch {
                epoch: 36,
                seconds: 18.5
            })
        );
        assert_eq!(
            parse(r#"@instantpad {"download": "rmvpe.pt", "done": 10, "size": 20}"#),
            Some(KitEvent::Download {
                file: "rmvpe.pt".to_owned(),
                done: 10,
                size: 20
            })
        );
        assert_eq!(
            parse(r#"@instantpad {"exported": "C:\\voices\\a\\app"}"#),
            Some(KitEvent::Exported(r"C:\voices\a\app".to_owned()))
        );
        assert_eq!(
            parse("@instantpad {\"error\": \"no recordings\"}\r\n"),
            Some(KitEvent::Error("no recordings".to_owned()))
        );
    }

    #[test]
    fn other_lines_are_no_events() {
        assert_eq!(
            parse("====> Epoch: 12 [2026-11-03 18:00:00] | (0:00:21)"),
            None
        );
        assert_eq!(parse("@instantpad not json"), None);
        assert_eq!(parse(r#"@instantpad {"step": "dance"}"#), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn the_card_with_the_most_memory_is_taken() {
        let output = "NVIDIA GeForce GT 1030, 2048, 610.88\r\n\
                      NVIDIA GeForce RTX 3070 Ti, 8192, 610.88\r\n";

        let card = graphics_card(output).unwrap();

        assert_eq!(card.name, "NVIDIA GeForce RTX 3070 Ti");
        assert_eq!(card.memory_megabytes, 8192);
        assert_eq!(card.driver, "610.88");
    }

    #[test]
    fn no_card_is_read_from_an_error_or_nothing() {
        assert_eq!(graphics_card(""), None);
        assert_eq!(
            graphics_card("NVIDIA-SMI has failed because it couldn't communicate"),
            None
        );
        assert_eq!(graphics_card("Some card, [N/A], 610.88"), None);
    }
}
