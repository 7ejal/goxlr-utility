use crate::daemon::{self, Live};
use anyhow::{Result, ensure};
use goxlr_ipc::GoXLRCommand;

#[derive(Clone)]
pub enum Action {
    Save,
    SaveAs(String),
    Load(String),
}

pub fn valid_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
            })
        });
    !name.is_empty()
        && name.trim() == name
        && !name.ends_with('.')
        && name.encode_utf16().count() <= 255 - ".goxlrMicProfile".len()
        && !reserved
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
}

pub fn command(expected: &Live, current: &Live, action: &Action) -> Result<(GoXLRCommand, String)> {
    ensure!(
        expected.key == current.key && expected.firmware == current.firmware,
        "Active device or microphone profile changed; refresh before proceeding"
    );
    ensure!(
        daemon::native_eq_writes_verified(&current.firmware),
        "Microphone profile operations require verified native EQ firmware"
    );
    ensure!(
        valid_name(&current.key.name),
        "Invalid active microphone profile name"
    );
    let (operation, target) = match action {
        Action::Save => (GoXLRCommand::SaveMicProfile(), current.key.name.clone()),
        Action::SaveAs(name) => {
            ensure!(
                valid_name(name),
                "Use a valid microphone profile filename without a path"
            );
            ensure!(
                !current
                    .mic_profiles
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(name)),
                "Microphone profile already exists; choose a different name"
            );
            (GoXLRCommand::SaveMicProfileAs(name.clone()), name.clone())
        }
        Action::Load(name) => {
            ensure!(
                valid_name(name) && current.mic_profiles.contains(name),
                "Select an existing microphone profile from the daemon list"
            );
            (
                GoXLRCommand::LoadMicProfile(name.clone(), true),
                name.clone(),
            )
        }
    };
    Ok((operation, target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProfileKey;

    fn live() -> Live {
        Live {
            key: ProfileKey {
                serial: "fixture".into(),
                name: "Current".into(),
            },
            firmware: daemon::DYNAMICS_FIRMWARE.into(),
            bands: crate::model::defaults(),
            dynamics: Some([0; 11]),
            mic_profiles: vec!["Current".into(), "Studio العربية".into()],
        }
    }

    #[test]
    fn profile_names_stay_inside_windows_profile_directory() {
        for name in [
            "",
            "..",
            ".",
            "../other",
            "C:\\other",
            "trailing.",
            " leading",
            "CON",
            "aux.xml",
            "LPT9",
            "bad?",
            "bad\n",
        ] {
            assert!(!valid_name(name), "Accepted {name:?}");
        }
        assert!(valid_name("Studio العربية"));
        assert!(valid_name("COM10"));
        assert!(!valid_name(&"a".repeat(240)));
    }

    #[test]
    fn profile_commands_require_fresh_device_and_real_profile_list() {
        let expected = live();
        assert!(matches!(
            command(&expected, &expected, &Action::Save).unwrap().0,
            GoXLRCommand::SaveMicProfile()
        ));
        let (load, name) =
            command(&expected, &expected, &Action::Load("Studio العربية".into())).unwrap();
        assert!(matches!(load, GoXLRCommand::LoadMicProfile(_, true)));
        assert_eq!(name, "Studio العربية");
        assert!(command(&expected, &expected, &Action::Load("unknown".into())).is_err());
        assert!(command(&expected, &expected, &Action::SaveAs("current".into())).is_err());
        assert!(command(&expected, &expected, &Action::SaveAs("../escape".into())).is_err());
        assert!(matches!(
            command(&expected, &expected, &Action::SaveAs("New".into()))
                .unwrap()
                .0,
            GoXLRCommand::SaveMicProfileAs(_)
        ));
        let mut changed = expected.clone();
        changed.key.name = "Other".into();
        assert!(command(&expected, &changed, &Action::Save).is_err());
        changed = expected.clone();
        changed.firmware = daemon::ARM_DIAGNOSTICS_FIRMWARE.into();
        assert!(command(&changed, &changed, &Action::Save).is_ok());
        changed.firmware = "1.5.1.911".into();
        assert!(command(&changed, &changed, &Action::Save).is_err());
    }
}
