use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AppTheme {
    #[default]
    Dark,
    Light,
}

#[derive(Clone, Copy)]
pub(crate) struct ThemePalette {
    pub(crate) window: u32,
    pub(crate) header: u32,
    pub(crate) toolbar: u32,
    pub(crate) panel: u32,
    pub(crate) card: u32,
    pub(crate) border: u32,
    pub(crate) foreground: u32,
    pub(crate) secondary_text: u32,
    pub(crate) muted_text: u32,
    pub(crate) accent: u32,
    pub(crate) status_error: u32,
    pub(crate) diff_equal: u32,
    pub(crate) diff_changed: u32,
    pub(crate) diff_added: u32,
    pub(crate) diff_removed: u32,
    pub(crate) diff_changed_text: u32,
    pub(crate) diff_added_text: u32,
    pub(crate) diff_removed_text: u32,
    pub(crate) left_only: u32,
    pub(crate) right_only: u32,
    pub(crate) identical: u32,
}

impl AppTheme {
    pub(crate) fn palette(self) -> ThemePalette {
        match self {
            Self::Dark => ThemePalette {
                window: 0x181818,
                header: 0x202020,
                toolbar: 0x252526,
                panel: 0x1e1e1e,
                card: 0x141920,
                border: 0x303030,
                foreground: 0xd4d4d4,
                secondary_text: 0x9da5b4,
                muted_text: 0x858585,
                accent: 0x007acc,
                status_error: 0xf48771,
                diff_equal: 0x1e1e1e,
                diff_changed: 0x302a1c,
                diff_added: 0x203a29,
                diff_removed: 0x402326,
                diff_changed_text: 0xe6cc8f,
                diff_added_text: 0xa8d8b7,
                diff_removed_text: 0xe8aaaa,
                left_only: 0xe8bd78,
                right_only: 0x8db7ef,
                identical: 0x8ebd8a,
            },
            Self::Light => ThemePalette {
                window: 0xf6f8fa,
                header: 0xffffff,
                toolbar: 0xeff2f5,
                panel: 0xffffff,
                card: 0xf2f4f7,
                border: 0xd0d7de,
                foreground: 0x1f2328,
                secondary_text: 0x59636e,
                muted_text: 0x6e7781,
                accent: 0x0969da,
                status_error: 0xcf222e,
                diff_equal: 0xffffff,
                diff_changed: 0xfff8c5,
                diff_added: 0xdafbe1,
                diff_removed: 0xffebe9,
                diff_changed_text: 0x825b00,
                diff_added_text: 0x116329,
                diff_removed_text: 0xcf222e,
                left_only: 0x9a6700,
                right_only: 0x0550ae,
                identical: 0x116329,
            },
        }
    }

    pub(crate) fn load_preference() -> io::Result<Option<Self>> {
        let path = preference_path()?;
        match fs::read_to_string(path) {
            Ok(value) => match value.trim() {
                "dark" => Ok(Some(Self::Dark)),
                "light" => Ok(Some(Self::Light)),
                value => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unrecognized theme preference: {value}"),
                )),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn save_preference(self) -> io::Result<()> {
        let path = preference_path()?;
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "theme preference path has no parent directory",
            )
        })?;
        fs::create_dir_all(parent)?;
        fs::write(
            path,
            match self {
                Self::Dark => "dark",
                Self::Light => "light",
            },
        )
    }
}

fn preference_path() -> io::Result<PathBuf> {
    #[cfg(target_os = "windows")]
    let directory = std::env::var_os("APPDATA").map(PathBuf::from);

    #[cfg(target_os = "macos")]
    let directory = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library").join("Application Support"));

    #[cfg(all(unix, not(target_os = "macos")))]
    let directory = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
        });

    directory
        .map(|directory| directory.join("Checker").join("theme"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not locate the application configuration directory",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::AppTheme;

    #[test]
    fn dark_theme_remains_the_default() {
        assert_eq!(AppTheme::default(), AppTheme::Dark);
    }

    #[test]
    fn light_and_dark_palettes_keep_distinct_surface_colors() {
        let dark = AppTheme::Dark.palette();
        let light = AppTheme::Light.palette();
        assert_ne!(dark.window, light.window);
        assert_ne!(dark.diff_changed, light.diff_changed);
        assert_ne!(dark.foreground, light.foreground);
    }
}
