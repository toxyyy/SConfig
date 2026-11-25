use crate::files::Contents;
use serde::Serialize;
#[derive(Serialize)]
pub struct Setting {
    pub label: String,
    pub value: String,
    pub group: String,
}
fn number(value: &str, whole: bool) -> String {
    let Ok(value) = value.parse::<f64>() else {
        return value.into();
    };
    if !value.is_finite() {
        return String::new();
    }
    if whole {
        return format!("{value:.0}");
    }
    let value = format!("{value:.6}");
    value
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
pub fn summarize(data: &Contents) -> Vec<Setting> {
    let mut pairs = vec![];
    for bytes in data.iter().flatten() {
        if bytes.len() > 1024 * 1024 {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim().trim_matches(',');
            if line.starts_with(['#', ';', '[']) {
                continue;
            }
            let Some((key, value)) = line
                .split_once('=')
                .or_else(|| line.split_once(':'))
                .or_else(|| line.split_once(char::is_whitespace))
            else {
                continue;
            };
            let key = key
                .trim()
                .trim_matches('"')
                .to_ascii_lowercase()
                .replace(['_', ' ', '.'], "");
            let value = value.trim().trim_matches(['"', ',']);
            if value.is_empty()
                || value.len() > 50
                || !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || " .-%_".contains(c))
            {
                continue;
            }
            pairs.push((key, value.to_owned()));
        }
    }
    let minecraft = pairs.iter().any(|(key, _)| key == "renderdistance")
        && pairs.iter().any(|(key, _)| key == "mousesensitivity");
    let advanced = pairs
        .iter()
        .any(|(k, v)| k == "adsmouseusespecific" && matches!(v.as_str(), "1" | "True" | "true"));
    let mut out = vec![];
    for (key, value) in &pairs {
        let mut group = "general";
        let label = match key.as_str() {
            "vsync" | "graphicsvsync" | "fullscreen" | "fullscreenmode" | "windowmode"
            | "graphicsscreenmode" => continue,
            "mouseyawsensitivity" => "Horizontal sensitivity",
            "mousepitchsensitivity" => "Vertical sensitivity",
            "mousesensitivity" | "sensitivity" | "inputsensitivity" => "Sensitivity",
            "mousesensitivitymultiplierunit" => "Sensitivity multiplier",
            "inputadssensitivity" => "ADS sensitivity",
            "adsmousesensitivityglobal" | "aimdownsightsmouse" => {
                if advanced {
                    group = "ads"
                };
                "ADS sensitivity"
            }
            "displaywidth" | "resolutionsizex" | "resx" | "resolutionwidth" | "fullscreenwidth"
            | "screenwidth" => "Width",
            "displayheight" | "resolutionsizey" | "resy" | "resolutionheight"
            | "fullscreenheight" | "screenheight" => "Height",
            "fov" if minecraft => "Field of view (normalized)",
            "graphicsfov" | "fov" | "defaultfov" | "fieldofview" => "Field of view",
            "frameratelimit" | "fpslimit" | "maxfps" | "framelimit" | "globalmaxfps" => "FPS limit",
            "refresh" | "refreshrate" => "Refresh rate",
            "overallqualitylevelname"
            | "graphicsquality"
            | "graphicsqualitylevel"
            | "graphicsqualitypreset"
            | "qualitylevel" => "Overall quality",
            "renderdistance" => {
                group = "graphics";
                "Render distance"
            }
            "graphicsmode" => {
                group = "graphics";
                "Graphics mode"
            }
            "graphicsdrawdistance" => {
                group = "graphics";
                "Draw distance"
            }
            "graphicsrenderscale" => {
                group = "graphics";
                "Render scale"
            }
            "graphicsdlss" => {
                group = "graphics";
                "DLSS level"
            }
            "graphicsaf" | "texturefiltering" => {
                group = "graphics";
                "Texture filtering"
            }
            "graphicsparallax" => {
                group = "graphics";
                "Parallax"
            }
            "texture" | "texturequality" | "graphicssettingstexturequalitypreset" => {
                group = "graphics";
                "Texture quality"
            }
            "shadow" | "shadowquality" | "graphicssettingsshadowqualitypreset" => {
                group = "graphics";
                "Shadow quality"
            }
            "shading" | "shadingquality" => {
                group = "graphics";
                "Shading quality"
            }
            "reflection" | "reflectionquality" => {
                group = "graphics";
                "Reflections"
            }
            "lod" | "lodquality" | "meshlodbias" => {
                group = "graphics";
                "Level of detail"
            }
            "grassquality" => {
                group = "graphics";
                "Grass quality"
            }
            "meshquality" => {
                group = "graphics";
                "Mesh quality"
            }
            "decorquality" => {
                group = "graphics";
                "Decor quality"
            }
            "particlesquality" => {
                group = "graphics";
                "Particle quality"
            }
            "adsmousesensitivity1x" if advanced => {
                group = "ads";
                "1x"
            }
            "adsmousesensitivity1xhalf" if advanced => {
                group = "ads";
                "1.5x"
            }
            "adsmousesensitivity2x" if advanced => {
                group = "ads";
                "2x"
            }
            "adsmousesensitivity2xhalf" if advanced => {
                group = "ads";
                "2.5x"
            }
            "adsmousesensitivity3x" if advanced => {
                group = "ads";
                "3x"
            }
            "adsmousesensitivity3xhalf" if advanced => {
                group = "ads";
                "3.5x"
            }
            "adsmousesensitivity4x" if advanced => {
                group = "ads";
                "4x"
            }
            "adsmousesensitivity5x" if advanced => {
                group = "ads";
                "5x"
            }
            "adsmousesensitivity8x" if advanced => {
                group = "ads";
                "8x"
            }
            "adsmousesensitivity12x" if advanced => {
                group = "ads";
                "12x"
            }
            "adsmousemultiplierunit" if advanced => {
                group = "ads";
                "ADS multiplier"
            }
            _ => continue,
        };
        if out
            .iter()
            .any(|s: &Setting| s.label == label && s.group == group)
        {
            continue;
        }
        let whole = matches!(
            label,
            "Field of view" | "Refresh rate" | "Width" | "Height" | "FPS limit"
        );
        let mut value = number(value, whole);
        if label == "FPS limit" && value == "0" {
            value = "Unlimited".into()
        }
        out.push(Setting {
            label: label.into(),
            value,
            group: group.into(),
        });
    }
    if out.iter().any(|s| s.group == "graphics")
        && !out.iter().any(|s| s.label == "Overall quality")
    {
        out.push(Setting {
            label: "Overall quality".into(),
            value: "Per-setting".into(),
            group: "general".into(),
        });
    }
    out
}
