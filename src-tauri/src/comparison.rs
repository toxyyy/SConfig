use serde::Serialize;

#[derive(Serialize)]
pub struct Line<'a> {
    pub added: bool,
    pub number: usize,
    pub text: &'a str,
}
#[derive(Serialize)]
pub struct Difference<'a> {
    pub file: &'a str,
    pub state: &'static str,
    pub lines: Vec<Line<'a>>,
    pub more: bool,
}

fn text_lines(bytes: Option<&[u8]>) -> Option<Vec<&str>> {
    let bytes = bytes.unwrap_or_default();
    if bytes.len() > 65536 {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return None;
    }
    Some(text.split_inclusive('\n').collect())
}

pub fn compare<'a>(file: &'a str, a: Option<&'a [u8]>, b: Option<&'a [u8]>) -> Difference<'a> {
    let state = if a == b {
        "Identical"
    } else if a.is_none() {
        "Added"
    } else if b.is_none() {
        "Removed"
    } else {
        "Modified"
    };
    let mut result = Difference {
        file,
        state,
        lines: vec![],
        more: false,
    };
    if a == b {
        return result;
    }
    let (Some(old), Some(new)) = (text_lines(a), text_lines(b)) else {
        return result;
    };
    let width = new.len() + 1;
    // Bound the longest-common-subsequence table for very large text files.
    if (old.len() + 1) * width > 300_000 {
        return result;
    }
    let mut lcs = vec![0u32; (old.len() + 1) * width];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            let k = i * width + j;
            lcs[k] = if old[i] == new[j] {
                1 + lcs[k + width + 1]
            } else {
                lcs[k + width].max(lcs[k + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            i += 1;
            j += 1;
            continue;
        }
        if result.lines.len() == 200 {
            result.more = true;
            break;
        }
        let add =
            i == old.len() || (j < new.len() && lcs[i * width + j + 1] > lcs[(i + 1) * width + j]);
        let line = if add {
            j += 1;
            Line {
                added: true,
                number: j,
                text: new[j - 1],
            }
        } else {
            i += 1;
            Line {
                added: false,
                number: i,
                text: old[i - 1],
            }
        };
        result.lines.push(line);
    }
    result
}
