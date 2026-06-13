use crate::bail;
use crate::error::Result;

use super::Config;

pub fn validate_config(config: &Config) -> Result<()> {
    let tables = config
        .ext
        .iter()
        .map(|(k, v)| (format!("ext.{k}"), v))
        .chain(config.protocol.iter().map(|(k, v)| (format!("protocol.{k}"), v)));

    for (name, table) in tables {
        if let Some(app) = &table.default {
            if !config.apps.contains_key(app) {
                bail!("[{name}] default の \"{app}\" が [apps] に定義されていません");
            }
        }
        if let Some(candidates) = &table.candidates {
            for app in candidates {
                if !config.apps.contains_key(app) {
                    bail!("[{name}] candidates の \"{app}\" が [apps] に定義されていません");
                }
            }
        }
        for (i, rule) in table.rules.iter().enumerate() {
            let at = format!("[{name}] rules[{i}]");
            match (&rule.app, rule.pick) {
                (Some(app), false) => {
                    if !config.apps.contains_key(app) {
                        bail!("{at}: app = \"{app}\" が [apps] に定義されていません");
                    }
                }
                (None, true) => {}
                (Some(_), true) => bail!("{at}: app と pick は同時指定できません"),
                (None, false) => bail!("{at}: app か pick = true のどちらかが必要です"),
            }
            if let Some(m) = &rule.modifier {
                if !matches!(m.as_str(), "shift" | "ctrl" | "alt") {
                    bail!("{at}: modifier は shift / ctrl / alt のいずれかです (指定値: {m})");
                }
            }
            if rule.glob.is_none() && rule.host.is_none() && rule.url.is_none() && rule.modifier.is_none() && i + 1 < table.rules.len() {
                bail!("{at}: 条件なしルール (catch-all) は最後にのみ置けます");
            }
        }
    }
    Ok(())
}
