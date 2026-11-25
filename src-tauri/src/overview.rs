use crate::files::Contents;
use serde::Serialize;
#[derive(Serialize)]
pub struct Setting {
    pub label: String,
    pub value: String,
}
pub fn summarize(data: &Contents) -> Vec<Setting> {
    let mut out = vec![];
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
            let key = key.trim().trim_matches('"');
            let normalized = key.to_ascii_lowercase().replace(['_', ' ', '.'], "");
            let label = match normalized.as_str() {
                "mouseyawsensitivity" => "Horizontal sensitivity",
                "mousepitchsensitivity" => "Vertical sensitivity",
                "mousesensitivity" | "sensitivity" | "inputsensitivity" => "Sensitivity",
                "mousesensitivitymultiplierunit" => "Sensitivity multiplier",
                "displaywidth" | "resolutionsizex" | "resx" | "resolutionwidth"
                | "fullscreenwidth" | "screenwidth" => "Width",
                "displayheight" | "resolutionsizey" | "resy" | "resolutionheight"
                | "fullscreenheight" | "screenheight" => "Height",
                "graphicsfov" | "fov" | "defaultfov" | "fieldofview" => "Field of view",
                "frameratelimit" | "fpslimit" | "maxfps" | "framelimit" => "FPS limit",
                "vsync" => "VSync",
                "renderdistance" => "Render distance",
                "graphicsmode" | "graphicsquality" => "Graphics",
                "fullscreenmode" | "fullscreen" | "windowmode" => "Display mode",
                "refresh" | "refreshrate" => "Refresh rate",
                _ => continue,
            };
            let value = value.trim().trim_matches(['"', ',']);
            if value.len() > 40
                || value.is_empty()
                || !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || " .-%_".contains(c))
            {
                continue;
            }
            if !out.iter().any(|v: &Setting| v.label == label) {
                out.push(Setting {
                    label: label.into(),
                    value: value.into(),
                })
            }
            if out.len() == 10 {
                return out;
            }
        }
    }
    out
}
