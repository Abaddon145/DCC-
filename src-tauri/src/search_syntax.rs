use rusqlite::types::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTerm {
    pub field: Option<String>,
    pub value: String,
    pub excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedQuery {
    pub groups: Vec<Vec<QueryTerm>>,
}

const FIELDS: &[&str] = &[
    "name", "tag", "desc", "category", "author", "software", "version", "format", "license",
    "source",
];

pub fn parse(raw: &str) -> Result<ParsedQuery, String> {
    let mut tokens = Vec::<String>::new();
    let mut token = String::new();
    let mut quoted = false;
    for ch in raw.trim().chars() {
        match ch {
            '"' => quoted = !quoted,
            '|' if !quoted => {
                if !token.trim().is_empty() {
                    tokens.push(token.trim().into());
                    token.clear();
                }
                tokens.push("|".into());
            }
            ch if ch.is_whitespace() && !quoted => {
                if !token.trim().is_empty() {
                    tokens.push(token.trim().into());
                    token.clear();
                }
            }
            _ => token.push(ch),
        }
    }
    if quoted {
        return Err("搜索语法错误：缺少结束引号".into());
    }
    if !token.trim().is_empty() {
        tokens.push(token.trim().into());
    }
    let mut groups = vec![Vec::new()];
    for mut token in tokens {
        if token == "|" {
            if groups.last().is_some_and(Vec::is_empty) {
                return Err("搜索语法错误：OR 两侧都需要条件".into());
            }
            groups.push(Vec::new());
            continue;
        }
        let excluded = token.starts_with('-');
        if excluded {
            token.remove(0);
        }
        if token.is_empty() {
            return Err("搜索语法错误：排除符号后缺少内容".into());
        }
        let (field, value) = if let Some((candidate, value)) = token.split_once(':') {
            let field = candidate.to_ascii_lowercase();
            if !FIELDS.contains(&field.as_str()) {
                return Err(format!("搜索字段不受支持：{candidate}"));
            }
            if value.trim().is_empty() {
                return Err(format!("搜索字段 {candidate}: 缺少内容"));
            }
            (Some(field), value.trim().to_string())
        } else {
            (None, token)
        };
        groups.last_mut().unwrap().push(QueryTerm {
            field,
            value,
            excluded,
        });
    }
    if groups.last().is_some_and(Vec::is_empty) && groups.len() > 1 {
        return Err("搜索语法错误：OR 后缺少条件".into());
    }
    Ok(ParsedQuery { groups })
}

pub fn simple_fts(query: &ParsedQuery) -> Option<&str> {
    let term = query.groups.first()?.first()?;
    (query.groups.len() == 1
        && query.groups[0].len() == 1
        && term.field.is_none()
        && !term.excluded)
        .then_some(term.value.as_str())
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn term_sql(term: &QueryTerm, _language: &str, values: &mut Vec<Value>) -> Result<String, String> {
    let needle = format!("%{}%", escape_like(&term.value.to_lowercase()));
    let marker = format!("?{}", values.len() + 1);
    let sql=match term.field.as_deref() {
        None => "(SELECT lower(text) FROM asset_search sx WHERE sx.asset_id=a.id) LIKE ? ESCAPE '\\'".into(),
        Some("name") => format!("(lower(a.name) LIKE {marker} ESCAPE '\\' OR EXISTS(SELECT 1 FROM asset_localizations al WHERE al.asset_id=a.id AND lower(al.name) LIKE {marker} ESCAPE '\\'))"),
        Some("desc") => format!("(lower(a.description) LIKE {marker} ESCAPE '\\' OR EXISTS(SELECT 1 FROM asset_localizations al WHERE al.asset_id=a.id AND lower(al.description) LIKE {marker} ESCAPE '\\'))"),
        Some("category") => "lower(COALESCE(c.name,'')) LIKE ? ESCAPE '\\'".into(),
        Some("author") => "lower(a.author) LIKE ? ESCAPE '\\'".into(),
        Some("source") => "lower(a.source_url || ' ' || a.share_url) LIKE ? ESCAPE '\\'".into(),
        Some("license") => format!("(lower(a.license) LIKE {marker} ESCAPE '\\' OR EXISTS(SELECT 1 FROM asset_localizations al WHERE al.asset_id=a.id AND lower(al.license) LIKE {marker} ESCAPE '\\'))"),
        Some("tag") => "EXISTS(SELECT 1 FROM asset_localized_tags at JOIN localized_tags t ON t.id=at.tag_id WHERE at.asset_id=a.id AND lower(t.name) LIKE ? ESCAPE '\\')".into(),
        Some("software") => "EXISTS(SELECT 1 FROM json_each(a.dcc_tools_json) j WHERE lower(j.value) LIKE ? ESCAPE '\\')".into(),
        Some("version") => "EXISTS(SELECT 1 FROM json_each(a.versions_json) j WHERE lower(j.value) LIKE ? ESCAPE '\\')".into(),
        Some("format") => "EXISTS(SELECT 1 FROM json_each(a.formats_json) j WHERE lower(j.value) LIKE ? ESCAPE '\\')".into(),
        _ => return Err("搜索字段不受支持".into()),
    };
    values.push(Value::Text(needle));
    Ok(if term.excluded {
        format!("NOT ({sql})")
    } else {
        sql
    })
}

pub fn sql(
    query: &ParsedQuery,
    language: &str,
    values: &mut Vec<Value>,
) -> Result<Option<String>, String> {
    if query.groups.is_empty() || query.groups[0].is_empty() {
        return Ok(None);
    }
    let mut groups = Vec::new();
    for group in &query.groups {
        let mut terms = Vec::new();
        for term in group {
            terms.push(term_sql(term, language, values)?);
        }
        groups.push(format!("({})", terms.join(" AND ")));
    }
    Ok(Some(format!("({})", groups.join(" OR "))))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_fields_phrases_or_and_exclusions() {
        let value = parse("name:\"desert dune\" tag:rock | -format:fbx").unwrap();
        assert_eq!(value.groups.len(), 2);
        assert_eq!(value.groups[0][0].value, "desert dune");
        assert!(value.groups[1][0].excluded);
    }
    #[test]
    fn rejects_unknown_fields_and_broken_quotes() {
        assert!(parse("nope:value").is_err());
        assert!(parse("\"broken").is_err());
    }
}
