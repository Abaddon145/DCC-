use super::transcription::*;
use serde_json::json;
use sha2::{Digest, Sha256};

const SUBTITLE: &str = "a26d86ee-3323-45cc-adc5-2e3693e3d4e7";

#[test]
fn learning_transcription_model_import_is_explicit_verified_and_never_overwrites() {
    use super::transcription_install::*;
    let (root, _tools, models, spec) = components();
    let source = root.path().join("原始模型.bin");
    std::fs::copy(models.join("ggml-base.bin"), &source).unwrap();
    let config = root.path().join("配置");
    import_model_with_spec(&config, &source, &spec.models[0]).unwrap();
    let target = WhisperPaths::new(&config).models.join("ggml-base.bin");
    assert_eq!(
        std::fs::read(&target).unwrap(),
        std::fs::read(&source).unwrap()
    );
    import_model_with_spec(&config, &source, &spec.models[0]).unwrap();
    std::fs::write(&target, b"existing damaged model").unwrap();
    assert!(import_model_with_spec(&config, &source, &spec.models[0]).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"existing damaged model");
    assert!(import_model(&config, "../unknown", &source).is_err());
    std::fs::write(&source, b"bad source").unwrap();
    let untouched = root.path().join("拒绝安装");
    assert!(import_model_with_spec(&untouched, &source, &spec.models[0]).is_err());
    assert!(!WhisperPaths::new(&untouched).models.exists());
}

#[test]
fn learning_transcription_tool_install_copies_only_pinned_components_and_license() {
    use super::transcription_install::*;
    let (root, tools, _models, mut spec) = components();
    let source = root.path().join("组件来源");
    std::fs::create_dir_all(source.join("tools")).unwrap();
    std::fs::create_dir_all(source.join("licenses")).unwrap();
    for name in spec.binary_hashes.keys() {
        std::fs::copy(tools.join(name), source.join("tools").join(name)).unwrap();
    }
    std::fs::write(
        source.join("licenses/whisper-MIT.txt"),
        b"synthetic license",
    )
    .unwrap();
    spec.license_hashes.insert(
        "whisper-MIT.txt".into(),
        format!("{:x}", Sha256::digest(b"synthetic license")),
    );
    std::fs::write(source.join("tools/untrusted.exe"), b"do not copy").unwrap();
    let config = root.path().join("配置");
    install_with_manifest(&config, &source, &spec).unwrap();
    let target = WhisperPaths::new(&config).tools;
    assert_eq!(
        std::fs::read(target.join("whisper-cli.exe")).unwrap(),
        b"synthetic exe"
    );
    assert_eq!(
        std::fs::read(target.join("whisper-MIT.txt")).unwrap(),
        b"synthetic license"
    );
    assert!(!target.join("untrusted.exe").exists());
    std::fs::write(target.join("ggml.dll"), b"keep existing damaged DLL").unwrap();
    assert!(install_with_manifest(&config, &source, &spec).is_err());
    assert_eq!(
        std::fs::read(target.join("ggml.dll")).unwrap(),
        b"keep existing damaged DLL"
    );
    std::fs::write(source.join("licenses/whisper-MIT.txt"), b"invalid license").unwrap();
    let other = root.path().join("拒绝工具");
    assert!(install_with_manifest(&other, &source, &spec).is_err());
    assert!(!WhisperPaths::new(&other).tools.exists());
}

fn rows(value: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"result":{"language":"zh"},"transcription":value})).unwrap()
}

#[test]
fn learning_transcription_keeps_complete_unicode_cues_and_srt_times() {
    let input = rows(json!([
        {"offsets":{"from":1100,"to":2900},"text":"学习🦀\n完整第二行"},
        {"offsets":{"from":4000,"to":5100},"text":"下一段"}
    ]));
    let parsed = parse_whisper_json(&input, SUBTITLE, 6000).unwrap();
    assert_eq!(parsed.language.as_deref(), Some("zh"));
    assert_eq!(parsed.cues.len(), 2);
    assert_eq!(parsed.cues[0].id, format!("{SUBTITLE}:0"));
    assert_eq!(parsed.cues[0].subtitle_id, SUBTITLE);
    assert_eq!(
        (parsed.cues[0].start_ms, parsed.cues[0].end_ms),
        (1100, 2900)
    );
    assert_eq!(parsed.cues[0].text, "学习🦀\n完整第二行");
    assert_eq!(String::from_utf8(transcript_srt(&parsed).unwrap()).unwrap(),
        "1\n00:00:01,100 --> 00:00:02,900\n学习🦀\n完整第二行\n\n2\n00:00:04,000 --> 00:00:05,100\n下一段\n\n");
    assert!(parse_whisper_json(&input, "../path", 6000).is_err());
}

#[test]
fn learning_transcription_never_clamps_skips_or_truncates_bad_rows() {
    let valid = json!({"offsets":{"from":1000,"to":2000},"text":"完整"});
    for bad in [
        json!({"offsets":{"from":-1,"to":2000},"text":"负数"}),
        json!({"offsets":{"from":1.5,"to":2000},"text":"小数"}),
        json!({"offsets":{"from":2000,"to":2000},"text":"零长"}),
        json!({"offsets":{"from":2000,"to":6001},"text":"越界"}),
        json!({"offsets":{"from":500,"to":800},"text":"乱序"}),
        json!({"offsets":{"from":2000,"to":3000},"text":"   "}),
        json!({"offsets":{"from":2000,"to":3000},"text":"\u{0}"}),
        json!({"offsets":{"from":2000,"to":3000},"text":"a".repeat(16001)}),
    ] {
        assert!(parse_whisper_json(&rows(json!([valid, bad])), SUBTITLE, 6000).is_err());
    }
    assert!(parse_whisper_json(&rows(json!([])), SUBTITLE, 6000).is_err());
    assert!(parse_whisper_json(&rows(json!([valid])), SUBTITLE, 0).is_err());
    assert!(parse_whisper_json(&rows(json!([valid])), SUBTITLE, 604800001).is_err());
    assert!(parse_whisper_json(&vec![b' '; 32 * 1024 * 1024 + 1], SUBTITLE, 6000).is_err());
    assert!(parse_whisper_json(&rows(json!(vec![valid; 200001])), SUBTITLE, 6000).is_err());
    assert!(parse_whisper_json(br#"{"transcription":["#, SUBTITLE, 6000).is_err());
}

#[test]
fn learning_transcription_refuses_lossy_srt_and_oversized_complete_output() {
    let input = rows(json!([{"offsets":{"from":1100,"to":2900},"text":"第一行\n\n第三行"}]));
    let parsed = parse_whisper_json(&input, SUBTITLE, 6000).unwrap();
    assert!(transcript_srt(&parsed).is_err());
    let input = rows(json!(vec![
        json!({"offsets":{"from":1100,"to":2900},"text":"a".repeat(16000)});
        525
    ]));
    let parsed = parse_whisper_json(&input, SUBTITLE, 6000).unwrap();
    assert!(transcript_srt(&parsed).is_err());
}

#[test]
fn learning_transcription_accepts_complete_srt_at_exact_eight_mib_limit() {
    // 523 * (16000 + 33) + digits(1..523)=8386720; final
    // three-digit cue adds 36 + 1852 bytes, reaching exactly 8388608.
    let mut value = vec![json!({"offsets":{"from":1100,"to":2900},"text":"a".repeat(16000)}); 523];
    value.push(json!({"offsets":{"from":3000,"to":4000},"text":"b".repeat(1852)}));
    let parsed = parse_whisper_json(&rows(json!(value)), SUBTITLE, 6000).unwrap();
    assert_eq!(transcript_srt(&parsed).unwrap().len(), 8388608);
}

// Micro files prove the production verifier's behavior. They never enter the
// compiled allowlist and are not executable binaries or usable speech models.
pub(super) fn components() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    WhisperManifest,
) {
    let root = tempfile::tempdir().unwrap();
    let tools = root.path().join("工具");
    let models = root.path().join("模型");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::create_dir_all(&models).unwrap();
    let exe = b"synthetic exe";
    let dll = b"synthetic dll";
    let mut model = vec![0u8; 48];
    for (n, v) in [
        (0usize, 0x67676d6cu32),
        (1, 51865),
        (2, 1500),
        (3, 512),
        (9, 8),
    ] {
        model[n * 4..n * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    std::fs::write(tools.join("whisper-cli.exe"), exe).unwrap();
    std::fs::write(tools.join("ggml.dll"), dll).unwrap();
    std::fs::write(models.join("ggml-base.bin"), &model).unwrap();
    let spec: WhisperManifest = serde_json::from_value(json!({
        "version":1,"whisperVersion":"1.9.1","modelRevision":"synthetic",
        "binaryHashes":{"whisper-cli.exe":format!("{:x}",Sha256::digest(exe)),"ggml.dll":format!("{:x}",Sha256::digest(dll))},
        "models":[{"id":"base","label":"synthetic","filename":"ggml-base.bin","bytes":48,"sha256":format!("{:x}",Sha256::digest(&model)),"url":"https://invalid.example/model","source":"synthetic","license":"MIT"}]
    })).unwrap();
    (root, tools, models, spec)
}

#[test]
fn learning_transcription_requires_every_dependency_and_full_model_digest() {
    let (_root, tools, models, spec) = components();
    let verified = verify_with_manifest(&tools, &models, "base", &spec, &|| false).unwrap();
    assert_eq!(verified.version, "1.9.1");
    assert_eq!(verified.executable.file_name().unwrap(), "whisper-cli.exe");
    assert_eq!(verified.model.file_name().unwrap(), "ggml-base.bin");
    assert_eq!(verified.model_sha256, spec.models[0].sha256);
    drop(verified);
    assert!(
        verify_with_manifest(&tools, &models, "base", &spec, &|| true)
            .err()
            .unwrap()
            .contains("取消")
    );
    std::fs::remove_file(tools.join("ggml.dll")).unwrap();
    assert!(verify_with_manifest(&tools, &models, "base", &spec, &|| false).is_err());
    std::fs::write(tools.join("ggml.dll"), b"synthetic dll").unwrap();
    let mut bytes = std::fs::read(models.join("ggml-base.bin")).unwrap();
    bytes[47] = 1;
    std::fs::write(models.join("ggml-base.bin"), &bytes).unwrap();
    assert!(verify_with_manifest(&tools, &models, "base", &spec, &|| false).is_err());
    assert!(verify_with_manifest(&tools, &models, "../base", &spec, &|| false).is_err());
}

#[test]
fn learning_transcription_rejects_unregistered_dlls_before_execution() {
    let (_root, tools, models, spec) = components();
    std::fs::write(tools.join("ggml-cpu-extra.DLL"), b"unregistered code").unwrap();
    assert!(verify_with_manifest(&tools, &models, "base", &spec, &|| false).is_err());
}

#[test]
fn learning_transcription_compiled_registry_does_not_discover_or_install_tools() {
    let root = tempfile::tempdir().unwrap();
    let tools = root.path().join("not-installed");
    let models = root.path().join("no-models");
    assert!(verify_components(&tools, &models, "../base", &|| false)
        .err()
        .unwrap()
        .contains("模型选项无效"));
    assert!(verify_components(&tools, &models, "base", &|| false)
        .err()
        .unwrap()
        .contains("组件缺失"));
    assert!(!tools.exists());
    assert!(!models.exists());
}

#[test]
fn learning_transcription_rejects_untrusted_paths_and_model_header() {
    let (_root, tools, models, mut spec) = components();
    for name in [
        "../outside.exe",
        "C:/outside.exe",
        "whisper-cli.exe:stream",
        "folder/exe",
        "exe\0",
    ] {
        spec.binary_hashes.insert(name.into(), "a".repeat(64));
        assert!(verify_with_manifest(&tools, &models, "base", &spec, &|| false).is_err());
        spec.binary_hashes.remove(name);
    }
    spec.models[0].filename = "../ggml-base.bin".into();
    assert!(verify_with_manifest(&tools, &models, "base", &spec, &|| false).is_err());
    spec.models[0].filename = "ggml-base.bin".into();
    let bytes = vec![0u8; 48];
    std::fs::write(models.join("ggml-base.bin"), &bytes).unwrap();
    spec.models[0].sha256 = format!("{:x}", Sha256::digest(&bytes));
    assert!(verify_with_manifest(&tools, &models, "base", &spec, &|| false).is_err());
}

#[test]
#[cfg(windows)]
fn learning_transcription_holds_read_locks_and_rejects_junctions() {
    let (root, tools, models, spec) = components();
    let held = verify_with_manifest(&tools, &models, "base", &spec, &|| false).unwrap();
    assert!(std::fs::OpenOptions::new()
        .write(true)
        .open(models.join("ggml-base.bin"))
        .is_err());
    assert!(std::fs::OpenOptions::new()
        .write(true)
        .open(tools.join("ggml.dll"))
        .is_err());
    drop(held);
    let link = root.path().join("junction");
    let status = std::process::Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&tools)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(verify_with_manifest(&link, &models, "base", &spec, &|| false).is_err());
    std::fs::remove_dir(&link).unwrap();
}
