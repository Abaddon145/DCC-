use crate::models::{TranslationTerm, TranslationTermImportReport, TranslationTermInput};
use calamine::{open_workbook_auto, Reader};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const GLOSSARY_VERSION: u32 = 1;

// Independently curated product and pipeline terminology. Product names are
// preserved; technical concepts use the common Simplified Chinese wording.
const TRANSLATED_TERMS: &[(&str, &str)] = &[
    ("Skeletal Mesh", "骨骼网格体"),
    ("Static Mesh", "静态网格体"),
    ("Instanced Static Mesh", "实例化静态网格体"),
    ("Hierarchical Instanced Static Mesh", "分层实例化静态网格体"),
    ("Material Instance", "材质实例"),
    ("Material Function", "材质函数"),
    ("Material Parameter Collection", "材质参数集合"),
    ("Virtual Shadow Maps", "虚拟阴影贴图"),
    ("Runtime Virtual Texture", "运行时虚拟纹理"),
    ("Virtual Texture", "虚拟纹理"),
    ("Global Illumination", "全局光照"),
    ("World Partition", "世界分区"),
    ("Level Instance", "关卡实例"),
    ("Data Layer", "数据层"),
    ("Content Browser", "内容浏览器"),
    ("Blueprint Function Library", "蓝图函数库"),
    ("Animation Blueprint", "动画蓝图"),
    ("Level Blueprint", "关卡蓝图"),
    ("Behavior Tree", "行为树"),
    ("Player Controller", "玩家控制器"),
    ("Geometry Collection", "几何体集合"),
    ("Texture Streaming", "纹理流送"),
    ("Mesh Distance Field", "网格体距离场"),
    ("Volumetric Fog", "体积雾"),
    ("Volumetric Cloud", "体积云"),
    ("Post Process Volume", "后期处理体积"),
    ("Motion Matching", "运动匹配"),
    ("Root Motion", "根运动"),
    ("Morph Target", "变形目标"),
    ("Movie Render Queue", "电影渲染队列"),
    ("Procedural Content Generation", "程序化内容生成"),
    ("Level Sequence", "关卡序列"),
    ("Blueprint", "蓝图"),
    ("Environment Asset", "环境素材"),
    ("Modular Environment", "模块化环境"),
    ("Texture", "纹理"),
    ("Material", "材质"),
    ("Shader", "着色器"),
    ("Mesh", "网格体"),
    ("Animation", "动画"),
    ("Character Rig", "角色绑定"),
    ("Landscape", "地形"),
    ("Foliage", "植被"),
    ("Decal", "贴花"),
    ("Lightmap", "光照贴图"),
    ("Retopology", "重拓扑"),
    ("UV Unwrapping", "UV 展开"),
    ("Texture Baking", "纹理烘焙"),
    ("Normal Map", "法线贴图"),
    ("Roughness Map", "粗糙度贴图"),
    ("Displacement Map", "置换贴图"),
    ("Ambient Occlusion", "环境光遮蔽"),
    ("Physically Based Rendering", "基于物理的渲染"),
    ("Ray Tracing", "光线追踪"),
    ("Path Tracing", "路径追踪"),
    ("Particle System", "粒子系统"),
    ("Visual Effects", "视觉特效"),
    ("Collision Geometry", "碰撞几何体"),
    ("Proxy Geometry", "代理几何体"),
    ("Procedural Modeling", "程序化建模"),
    ("Node Graph", "节点图"),
    ("Scene Graph", "场景图"),
    ("Digital Asset", "数字资产"),
    ("Height Field", "高度场"),
    ("Rigid Body", "刚体"),
    ("Soft Body", "软体"),
    ("Fluid Simulation", "流体模拟"),
    ("Cloth Simulation", "布料模拟"),
    ("Color Grading", "颜色分级"),
    ("Tone Mapping", "色调映射"),
];

const PRESERVED_TERMS: &[&str] = &[
    "Unreal Engine",
    "Unreal Editor",
    "UE",
    "UE4",
    "UE5",
    "Fab",
    "Megascans",
    "Quixel",
    "Nanite",
    "Lumen",
    "Niagara",
    "MetaHuman",
    "Control Rig",
    "MetaSounds",
    "Blender",
    "Maya",
    "3ds Max",
    "Houdini",
    "Houdini Engine",
    "Substance 3D",
    "Substance Painter",
    "Substance Designer",
    "ZBrush",
    "Cinema 4D",
    "Marvelous Designer",
    "SpeedTree",
    "OpenUSD",
    "USD",
    "FBX",
    "OBJ",
    "glTF",
    "GLB",
    "PBR",
    "LOD",
    "HLOD",
    "PCG",
    "VFX",
    "Alembic",
    "MaterialX",
    "OpenVDB",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlossaryFile {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    custom_terms: Vec<TranslationTerm>,
    #[serde(default)]
    disabled_builtins: Vec<String>,
}

fn default_version() -> u32 {
    GLOSSARY_VERSION
}

impl Default for GlossaryFile {
    fn default() -> Self {
        Self {
            version: GLOSSARY_VERSION,
            custom_terms: Vec::new(),
            disabled_builtins: Vec::new(),
        }
    }
}

pub struct GlossaryStore {
    path: PathBuf,
    data: GlossaryFile,
}

impl GlossaryStore {
    pub fn load(path: PathBuf) -> Self {
        let data = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<GlossaryFile>(&bytes).ok())
            .unwrap_or_default();
        Self { path, data }
    }

    pub fn list(
        &self,
        query: Option<&str>,
        source_language: Option<&str>,
        target_language: Option<&str>,
        origin: Option<&str>,
        enabled: Option<bool>,
    ) -> Vec<TranslationTerm> {
        let needle = query.unwrap_or_default().trim().to_lowercase();
        let mut terms = self.all_terms();
        terms.retain(|term| {
            (needle.is_empty()
                || term.source.to_lowercase().contains(&needle)
                || term.target.to_lowercase().contains(&needle))
                && source_language.is_none_or(|value| term.source_language == value)
                && target_language.is_none_or(|value| term.target_language == value)
                && origin.is_none_or(|value| term.origin == value)
                && enabled.is_none_or(|value| term.enabled == value)
        });
        terms.sort_by(|a, b| {
            a.source_language
                .cmp(&b.source_language)
                .then_with(|| a.source.to_lowercase().cmp(&b.source.to_lowercase()))
                .then_with(|| a.origin.cmp(&b.origin))
        });
        terms
    }

    pub fn effective(&self, source_language: &str, target_language: &str) -> Vec<TranslationTerm> {
        let mut custom_keys = HashSet::new();
        for term in &self.data.custom_terms {
            if term.enabled
                && term.source_language == source_language
                && term.target_language == target_language
            {
                custom_keys.insert(natural_key(term));
            }
        }
        let mut terms = self
            .data
            .custom_terms
            .iter()
            .filter(|term| {
                term.enabled
                    && term.source_language == source_language
                    && term.target_language == target_language
            })
            .cloned()
            .collect::<Vec<_>>();
        terms.extend(builtin_terms().into_iter().filter(|term| {
            term.enabled
                && term.source_language == source_language
                && term.target_language == target_language
                && !self.data.disabled_builtins.contains(&term.id)
                && !custom_keys.contains(&natural_key(term))
        }));
        terms.sort_by(|a, b| {
            b.source
                .chars()
                .count()
                .cmp(&a.source.chars().count())
                .then_with(|| b.origin.cmp(&a.origin))
        });
        terms
    }

    pub fn upsert(&mut self, input: TranslationTermInput) -> Result<TranslationTerm, String> {
        let term = validate_input(input)?;
        let key = natural_key(&term);
        let existing = self
            .data
            .custom_terms
            .iter()
            .position(|value| value.id == term.id || natural_key(value) == key);
        let result = if let Some(index) = existing {
            let mut value = term;
            value.id = self.data.custom_terms[index].id.clone();
            self.data.custom_terms[index] = value.clone();
            value
        } else {
            self.data.custom_terms.push(term.clone());
            term
        };
        self.save()?;
        Ok(result)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let before = self.data.custom_terms.len();
        self.data.custom_terms.retain(|term| term.id != id);
        if self.data.custom_terms.len() == before {
            return Err("只能删除自定义术语".into());
        }
        self.save()
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        if id.starts_with("builtin-") {
            if !builtin_terms().iter().any(|term| term.id == id) {
                return Err("内置术语不存在".into());
            }
            self.data.disabled_builtins.retain(|value| value != id);
            if !enabled {
                self.data.disabled_builtins.push(id.to_string());
            }
        } else if let Some(term) = self.data.custom_terms.iter_mut().find(|term| term.id == id) {
            term.enabled = enabled;
        } else {
            return Err("术语不存在".into());
        }
        self.save()
    }

    pub fn reset(&mut self) -> Result<(), String> {
        self.data = GlossaryFile::default();
        self.save()
    }

    pub fn export_xlsx(&self, path: &Path) -> Result<(), String> {
        ensure_xlsx(path)?;
        let rows = self.all_terms();
        let file = File::create(path).map_err(|error| format!("创建术语表失败：{error}"))?;
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in workbook_files(glossary_sheet_xml(&rows)) {
            archive
                .start_file(name, options)
                .map_err(|error| error.to_string())?;
            archive
                .write_all(content.as_bytes())
                .map_err(|error| error.to_string())?;
        }
        archive.finish().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn import_xlsx(&mut self, path: &Path) -> Result<TranslationTermImportReport, String> {
        ensure_xlsx(path)?;
        let mut workbook =
            open_workbook_auto(path).map_err(|error| format!("读取术语表失败：{error}"))?;
        let range = workbook
            .worksheet_range_at(0)
            .ok_or("术语表没有工作表")?
            .map_err(|error| format!("读取术语表失败：{error}"))?;
        let mut rows = range.rows();
        let headers = rows
            .next()
            .ok_or("术语表为空")?
            .iter()
            .enumerate()
            .map(|(index, value)| (value.to_string().trim().to_lowercase(), index))
            .collect::<HashMap<_, _>>();
        for required in [
            "source_language",
            "target_language",
            "source",
            "target",
            "mode",
        ] {
            if !headers.contains_key(required) {
                return Err(format!("术语表缺少列：{required}"));
            }
        }
        let mut report = TranslationTermImportReport::default();
        for (offset, row) in rows.enumerate() {
            let value = |name: &str| -> String {
                headers
                    .get(name)
                    .and_then(|index| row.get(*index))
                    .map(|cell| cell.to_string().trim().to_string())
                    .unwrap_or_default()
            };
            let source = value("source");
            if source.is_empty() {
                report.skipped += 1;
                continue;
            }
            let id = value("id");
            let origin = value("origin");
            let enabled = parse_bool(&value("enabled"), true);
            if origin == "builtin" && id.starts_with("builtin-") {
                if builtin_terms().iter().any(|term| term.id == id) {
                    self.data.disabled_builtins.retain(|item| item != &id);
                    if !enabled {
                        self.data.disabled_builtins.push(id);
                    }
                    report.updated += 1;
                } else {
                    report.skipped += 1;
                    report
                        .warnings
                        .push(format!("第 {} 行：内置术语 ID 已失效", offset + 2));
                }
                continue;
            }
            let input = TranslationTermInput {
                id: (!id.is_empty()).then_some(id),
                source_language: value("source_language"),
                target_language: value("target_language"),
                source,
                target: value("target"),
                mode: value("mode"),
                case_sensitive: parse_bool(&value("case_sensitive"), false),
                enabled,
            };
            match validate_input(input) {
                Ok(term) => {
                    let key = natural_key(&term);
                    if let Some(index) = self
                        .data
                        .custom_terms
                        .iter()
                        .position(|item| item.id == term.id || natural_key(item) == key)
                    {
                        let mut value = term;
                        value.id = self.data.custom_terms[index].id.clone();
                        self.data.custom_terms[index] = value;
                        report.updated += 1;
                    } else {
                        self.data.custom_terms.push(term);
                        report.imported += 1;
                    }
                }
                Err(error) => {
                    report.skipped += 1;
                    report
                        .warnings
                        .push(format!("第 {} 行：{error}", offset + 2));
                }
            }
        }
        self.save()?;
        Ok(report)
    }

    fn all_terms(&self) -> Vec<TranslationTerm> {
        let disabled = self.data.disabled_builtins.iter().collect::<HashSet<_>>();
        let mut terms = builtin_terms();
        for term in &mut terms {
            term.enabled = !disabled.contains(&term.id);
        }
        terms.extend(self.data.custom_terms.clone());
        terms
    }

    fn save(&self) -> Result<(), String> {
        let temporary = self.path.with_extension("tmp");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(&self.data).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("保存术语库失败：{error}"))?;
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|error| format!("更新术语库失败：{error}"))?;
        }
        fs::rename(&temporary, &self.path).map_err(|error| format!("更新术语库失败：{error}"))
    }
}

fn validate_input(input: TranslationTermInput) -> Result<TranslationTerm, String> {
    if !matches!(input.source_language.as_str(), "zh-CN" | "en")
        || !matches!(input.target_language.as_str(), "zh-CN" | "en")
        || input.source_language == input.target_language
    {
        return Err("术语只支持中文与英文互译".into());
    }
    let source = input.source.trim();
    let mut target = input.target.trim().to_string();
    if source.is_empty() {
        return Err("原词不能为空".into());
    }
    if !matches!(input.mode.as_str(), "translate" | "preserve") {
        return Err("术语模式无效".into());
    }
    if input.mode == "preserve" {
        target = source.to_string();
    } else if target.is_empty() {
        return Err("强制翻译术语必须填写目标词".into());
    }
    Ok(TranslationTerm {
        id: input
            .id
            .filter(|value| value.starts_with("custom-"))
            .unwrap_or_else(|| format!("custom-{}", Uuid::new_v4())),
        source_language: input.source_language,
        target_language: input.target_language,
        source: source.to_string(),
        target,
        mode: input.mode,
        case_sensitive: input.case_sensitive,
        enabled: input.enabled,
        origin: "custom".into(),
    })
}

fn natural_key(term: &TranslationTerm) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        term.source_language,
        term.target_language,
        term.source.trim().to_lowercase()
    )
}

fn builtin_terms() -> Vec<TranslationTerm> {
    let mut result = Vec::new();
    for (english, chinese) in TRANSLATED_TERMS {
        result.push(builtin("en", "zh-CN", english, chinese, "translate"));
        result.push(builtin("zh-CN", "en", chinese, english, "translate"));
    }
    for value in PRESERVED_TERMS {
        result.push(builtin("en", "zh-CN", value, value, "preserve"));
        result.push(builtin("zh-CN", "en", value, value, "preserve"));
    }
    result
}

fn builtin(
    source_language: &str,
    target_language: &str,
    source: &str,
    target: &str,
    mode: &str,
) -> TranslationTerm {
    let digest = format!(
        "{:x}",
        md5::compute(format!(
            "{source_language}\u{1f}{target_language}\u{1f}{source}"
        ))
    );
    TranslationTerm {
        id: format!(
            "builtin-{}-{}-{}",
            source_language.replace('-', ""),
            target_language.replace('-', ""),
            &digest[..12]
        ),
        source_language: source_language.into(),
        target_language: target_language.into(),
        source: source.into(),
        target: target.into(),
        mode: mode.into(),
        case_sensitive: false,
        enabled: true,
        origin: "builtin".into(),
    }
}

fn parse_bool(value: &str, default: bool) -> bool {
    match value.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "是" | "启用" => true,
        "false" | "0" | "no" | "否" | "停用" => false,
        _ => default,
    }
}

fn ensure_xlsx(path: &Path) -> Result<(), String> {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_none_or(|value| !value.eq_ignore_ascii_case("xlsx"))
    {
        Err("术语表必须使用 .xlsx 格式".into())
    } else {
        Ok(())
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn cell(column: char, row: usize, value: &str) -> String {
    format!(
        "<c r=\"{column}{row}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
        xml_escape(value)
    )
}

fn glossary_sheet_xml(terms: &[TranslationTerm]) -> String {
    let headers = [
        "id",
        "origin",
        "source_language",
        "target_language",
        "source",
        "target",
        "mode",
        "case_sensitive",
        "enabled",
    ];
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><cols><col min="1" max="2" width="24" customWidth="1"/><col min="3" max="4" width="18" customWidth="1"/><col min="5" max="6" width="34" customWidth="1"/><col min="7" max="9" width="16" customWidth="1"/></cols><sheetData>"#,
    );
    xml.push_str("<row r=\"1\">");
    for (index, header) in headers.iter().enumerate() {
        xml.push_str(&cell((b'A' + index as u8) as char, 1, header));
    }
    xml.push_str("</row>");
    for (offset, term) in terms.iter().enumerate() {
        let row = offset + 2;
        let values = [
            term.id.as_str(),
            term.origin.as_str(),
            term.source_language.as_str(),
            term.target_language.as_str(),
            term.source.as_str(),
            term.target.as_str(),
            term.mode.as_str(),
            if term.case_sensitive { "true" } else { "false" },
            if term.enabled { "true" } else { "false" },
        ];
        xml.push_str(&format!("<row r=\"{row}\">"));
        for (index, value) in values.iter().enumerate() {
            xml.push_str(&cell((b'A' + index as u8) as char, row, value));
        }
        xml.push_str("</row>");
    }
    xml.push_str("</sheetData></worksheet>");
    xml
}

fn workbook_files(sheet: String) -> Vec<(&'static str, String)> {
    vec![
        ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.into()),
        ("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.into()),
        ("xl/workbook.xml", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="专业术语库" sheetId="1" r:id="rId1"/></sheets></workbook>"#.into()),
        ("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.into()),
        ("xl/worksheets/sheet1.xml", sheet),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn custom_terms_override_builtins_and_longest_terms_sort_first() {
        let directory = tempdir().unwrap();
        let mut store = GlossaryStore::load(directory.path().join("translation-glossary.json"));
        store
            .upsert(TranslationTermInput {
                id: None,
                source_language: "en".into(),
                target_language: "zh-CN".into(),
                source: "Skeletal Mesh".into(),
                target: "骨架模型".into(),
                mode: "translate".into(),
                case_sensitive: false,
                enabled: true,
            })
            .unwrap();
        let terms = store.effective("en", "zh-CN");
        let matches = terms
            .iter()
            .filter(|term| term.source.eq_ignore_ascii_case("Skeletal Mesh"))
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].target, "骨架模型");
        assert_eq!(matches[0].origin, "custom");
        assert!(terms[0].source.chars().count() >= terms.last().unwrap().source.chars().count());
    }

    #[test]
    fn exports_and_imports_xlsx_overrides() {
        let directory = tempdir().unwrap();
        let mut source = GlossaryStore::load(directory.path().join("source.json"));
        source
            .upsert(TranslationTermInput {
                id: None,
                source_language: "en".into(),
                target_language: "zh-CN".into(),
                source: "Hero Asset".into(),
                target: "英雄资产".into(),
                mode: "translate".into(),
                case_sensitive: false,
                enabled: true,
            })
            .unwrap();
        let workbook = directory.path().join("terms.xlsx");
        source.export_xlsx(&workbook).unwrap();
        let mut target = GlossaryStore::load(directory.path().join("target.json"));
        let report = target.import_xlsx(&workbook).unwrap();
        assert!(report.imported >= 1);
        assert!(target
            .effective("en", "zh-CN")
            .iter()
            .any(|term| term.source == "Hero Asset" && term.target == "英雄资产"));
    }

    #[test]
    fn corrupted_file_falls_back_without_crashing() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("translation-glossary.json");
        fs::write(&path, b"not-json").unwrap();
        let store = GlossaryStore::load(path);
        assert!(!store.effective("en", "zh-CN").is_empty());
    }
}
