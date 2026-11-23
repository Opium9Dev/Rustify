use super::super::*;

#[derive(Clone, Default)]
pub(in crate::app) struct Hit {
    pub(in crate::app) title: String,
    pub(in crate::app) artist: String,
    pub(in crate::app) album: String,
    pub(in crate::app) secs: u32,
    pub(in crate::app) id: String,
    pub(in crate::app) thumb: String,
}

pub(in crate::app) type SearchMsg = (String, Result<Vec<Hit>, String>);

fn find_items<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if k == "musicResponsiveListItemRenderer" {
                    out.push(x);
                } else {
                    find_items(x, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| find_items(x, out)),
        _ => {}
    }
}

fn parse_item(it: &Value) -> Option<Hit> {
    let col = |i: usize| -> Vec<String> {
        it["flexColumns"][i]["musicResponsiveListItemFlexColumnRenderer"]["text"]["runs"]
            .as_array()
            .map(|a| a.iter().filter_map(|r| r["text"].as_str().map(String::from)).collect())
            .unwrap_or_default()
    };
    let title = col(0).into_iter().next()?;
    let id = it["playlistItemData"]["videoId"].as_str()?.to_string();

    let mut groups: Vec<String> = vec![String::new()];
    for r in col(1) {
        if r.trim() == "•" {
            groups.push(String::new());
        } else {
            groups.last_mut().unwrap().push_str(&r);
        }
    }
    let secs = groups
        .last()
        .filter(|g| g.contains(':'))
        .map(|g| {
            g.split(':')
                .fold(0u32, |a, p| a * 60 + p.trim().parse::<u32>().unwrap_or(0))
        })
        .unwrap_or(0);
    if secs > 0 {
        groups.pop();
    }
    let artist = groups.first().cloned().unwrap_or_default();
    let album = if groups.len() >= 2 {
        groups[1].clone()
    } else {
        String::new()
    };

    let mut thumb = it["thumbnail"]["musicThumbnailRenderer"]["thumbnail"]["thumbnails"][0]["url"]
        .as_str()
        .unwrap_or("")
        .to_string();
    if thumb.contains("googleusercontent.com") || thumb.contains("ggpht.com") {
        if let Some(i) = thumb.rfind('=') {
            thumb.truncate(i);
            thumb.push_str("=w96-h96-l90-rj");
        }
    }
    Some(Hit {
        title,
        artist,
        album,
        secs,
        id,
        thumb,
    })
}

pub(in crate::app) fn yt_search(q: &str) -> Result<Vec<Hit>, String> {
    let body = json!({
        "context": { "client": {
            "clientName": "WEB_REMIX", "clientVersion": "1.20250101.01.00", "hl": "en", "gl": "US"
        }},
        "query": q,
        "params": "EgWKAQIIAWoMEA4QChADEAQQCRAF"
    });
    let mut r = ureq::post("https://music.youtube.com/youtubei/v1/search?prettyPrint=false")
        .header("Origin", "https://music.youtube.com")
        .header(
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0",
        )
        .send_json(body)
        .map_err(|e| e.to_string())?;
    let resp: Value = r.body_mut().read_json().map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    find_items(&resp, &mut items);
    Ok(items.into_iter().filter_map(parse_item).collect())
}
