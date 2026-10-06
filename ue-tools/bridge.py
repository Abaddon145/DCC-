"""Fixed UE-native job entry. The desktop backend supplies the validated job file.

Do not run inside a user's project: index mounts metadata in a helper project;
render/preview/migrate operate on staged native package copies.
"""
import json
import re
import traceback
from pathlib import Path

import unreal


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".partial")
    temporary.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
    temporary.replace(path)


def run(job):
    mode = job["operation"]
    root = Path(job["contentRoot"]).resolve(strict=True)
    if not unreal.StackThumbnailLibrary.mount_game_content(str(root)):
        raise RuntimeError("Cannot mount the confirmed Content directory")
    registry = unreal.AssetRegistryHelpers.get_asset_registry()
    registry.scan_paths_synchronous(["/Game"], True)
    records = []
    for data in registry.get_assets_by_path("/Game", True):
        package = str(data.package_name)
        relative = package.removeprefix("/Game/")
        file = root / (relative + ".uasset")
        if not file.is_file():
            file = root / (relative + ".umap")
        if not file.is_file() or not file.resolve().is_relative_to(root):
            continue
        records.append({
            "packageName": package, "assetName": str(data.asset_name),
            "className": str(data.asset_class_path.asset_name),
            "objectPath": package + "." + str(data.asset_name),
            "relativePath": file.relative_to(root).as_posix(),
            "dependencies": sorted(str(item) for item in registry.get_dependencies(
                package, unreal.AssetRegistryDependencyOptions())),
        })
    if mode == "index":
        known = {record["relativePath"] for record in records}
        unresolved = []
        for file in root.rglob("*"):
            if file.suffix.lower() not in {".uasset", ".umap"} or not file.is_file():
                continue
            if not file.resolve().is_relative_to(root):
                raise RuntimeError("A package points outside the confirmed Content directory")
            relative = file.relative_to(root).as_posix()
            if relative not in known:
                unresolved.append({"relativePath": relative, "message": "匹配引擎未能读取此包；请检查版本、损坏文件或插件依赖"})
            if len(records) + len(unresolved) > 100000:
                raise RuntimeError("The source exceeds 100000 native entries")
        return {"assets": records, "unresolvedFiles": unresolved, "engineVersion": unreal.SystemLibrary.get_engine_version()}
    expected = {item["objectPath"] for item in job["assets"]}
    available = {item["objectPath"] for item in records}
    if not expected.issubset(available):
        raise RuntimeError("Staged packages differ from the confirmed native asset selection")
    if mode == "render":
        results = []
        for item in job["assets"]:
            succeeded = unreal.StackThumbnailLibrary.export_thumbnail_png(item["objectPath"], item["output"], 256)
            results.append({"id": item["id"], "succeeded": succeeded})
            write_json(job["progress"], {"current": len(results), "total": len(expected), "name": item["objectPath"]})
        return {"results": results}
    if mode == "preview":
        assets = [unreal.load_asset(item["objectPath"]) for item in job["assets"]]
        if any(asset is None for asset in assets):
            raise RuntimeError("A native package cannot be loaded; check engine/plugins/dependencies")
        editors = unreal.get_editor_subsystem(unreal.AssetEditorSubsystem)
        if not editors.open_editor_for_assets(assets) or not expected.issubset(set(unreal.StackThumbnailLibrary.get_open_native_asset_paths())):
            raise RuntimeError("UE did not open all requested native asset editors")
        unreal.EditorPythonScripting.set_keep_python_script_alive(True)
        return {"opened": True, "assets": sorted(expected)}
    if mode == "migrate":
        assets = [unreal.load_asset(item["objectPath"]) for item in job["assets"]]
        if any(asset is None for asset in assets):
            raise RuntimeError("Native validation failed before migration")
        options = unreal.MigrationOptions(prompt=False, ignore_dependencies=False, asset_conflict=unreal.AssetMigrationConflict.SKIP)
        unreal.AssetToolsHelpers.get_asset_tools().migrate_packages(
            [item["packageName"] for item in job["assets"]], job["destination"], options)
        return {"migrated": True}
    raise RuntimeError("Unsupported native operation")


match = re.search(r'(?:^|\s)-StackJob=(?:"([^"]+)"|(\S+))', unreal.SystemLibrary.get_command_line())
if not match:
    raise RuntimeError("Missing fixed native job manifest")
job = json.loads(Path(match.group(1) or match.group(2)).read_text(encoding="utf-8"))
try:
    result = run(job)
    write_json(job["report"], {"ok": True, "result": result})
except Exception as error:
    write_json(job["report"], {"ok": False, "error": str(error)})
    unreal.log_error(traceback.format_exc())
