use std::sync::LazyLock;

use nsh_base::Theme;
use nsh_core::{element::Appearance, prelude::AdaptiveColor};
use serde::Deserialize;

const SOURCE: &str = include_str!("../../design/tokens.json");
const COPPER: &str = include_str!("../../design/theme-verdigris.generated.json");
const AMBER: &str = include_str!("../../design/theme-amber.generated.json");

#[derive(Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Accent {
    Verdigris,
    Amber,
}

impl Accent {
    pub(super) fn theme(self) -> &'static Theme {
        static VERDIGRIS: LazyLock<Theme> =
            LazyLock::new(|| Theme::from_json(COPPER).expect("generated verdigris theme"));
        static AMBER_THEME: LazyLock<Theme> =
            LazyLock::new(|| Theme::from_json(AMBER).expect("generated amber theme"));
        match self {
            Self::Verdigris => &VERDIGRIS,
            Self::Amber => &AMBER_THEME,
        }
    }

    pub(super) fn toggle(self) -> Self {
        match self {
            Self::Verdigris => Self::Amber,
            Self::Amber => Self::Verdigris,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Density {
    Comfortable,
    Compact,
}

impl Density {
    pub(super) fn metrics(self) -> &'static DensityMetrics {
        match self {
            Self::Comfortable => &settings().density.comfortable,
            Self::Compact => &settings().density.compact,
        }
    }

    pub(super) fn toggle(self) -> Self {
        match self {
            Self::Comfortable => Self::Compact,
            Self::Compact => Self::Comfortable,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct DensityMetrics {
    #[serde(rename = "rowMin")]
    pub row: f32,
    #[serde(rename = "controlMin")]
    pub control: f32,
    #[serde(rename = "statusMin")]
    pub status: f32,
    pub gutter: f32,
}

#[derive(Deserialize)]
struct Densities {
    compact: DensityMetrics,
    comfortable: DensityMetrics,
}

#[derive(Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Scheme {
    System,
    Light,
    Dark,
}

impl From<Scheme> for Appearance {
    fn from(value: Scheme) -> Self {
        match value {
            Scheme::System => Self::System,
            Scheme::Light => Self::Light,
            Scheme::Dark => Self::Dark,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct Defaults {
    pub appearance: Scheme,
    pub accent: Accent,
    pub density: Density,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Shape {
    pub control: f32,
    pub card: f32,
    pub overlay: f32,
    pub focus_width: f32,
    pub focus_offset: f32,
}

#[derive(Deserialize)]
pub(super) struct Sizes {
    pub split: f32,
    pub wide: f32,
    pub rail: f32,
    pub detail: f32,
}

#[derive(Deserialize)]
pub(super) struct Space {
    pub step: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Motion {
    pub feedback_ms: f32,
    pub overlay_ms: f32,
    pub reduced_ms: f32,
    pub ease_out: [f32; 4],
}

#[derive(Deserialize)]
pub(super) struct Scale {
    pub meta: (f32, f32),
    pub body: (f32, f32),
    pub title: (f32, f32),
    pub page: (f32, f32),
}

#[derive(Deserialize)]
pub(super) struct Weights {
    pub regular: u16,
    pub emphasis: u16,
}

#[derive(Deserialize)]
pub(super) struct Typography {
    #[serde(deserialize_with = "font_family")]
    pub body: String,
    #[serde(deserialize_with = "font_family")]
    pub mono: String,
    pub scale: Scale,
    pub weights: Weights,
}

fn font_family<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let families = Vec::<String>::deserialize(deserializer)?;
    Ok(families.join(", "))
}

#[derive(Deserialize)]
pub(super) struct Settings {
    pub defaults: Defaults,
    pub shape: Shape,
    pub size: Sizes,
    pub space: Space,
    pub motion: Motion,
    #[serde(rename = "type")]
    pub typography: Typography,
    density: Densities,
}

pub(super) fn settings() -> &'static Settings {
    static SETTINGS: LazyLock<Settings> =
        LazyLock::new(|| serde_json::from_str(SOURCE).expect("validated NSH design source"));
    &SETTINGS
}

pub(super) fn color(role: &str) -> AdaptiveColor {
    nsh_base::theme()
        .color(role)
        .expect("generated design color role")
}
