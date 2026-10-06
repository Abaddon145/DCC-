"""Verify native dependency migration using only the disposable fixture projects."""
import hashlib
import json
from pathlib import Path

import unreal


project = Path(unreal.Paths.project_dir()).resolve()
names = ["VerifyMesh", "VerifyTexture", "VerifyMaterial", "VerifyMaterialInstance"]
packages = ["/Game/StackVerification/" + name for name in names]

if project.name == "ue-native-smoke":
    destination = project.parent / "ue-native-target" / "Content"
    destination.mkdir(parents=True, exist_ok=True)
    source_files = [project / "Content" / "StackVerification" / (name + ".uasset") for name in names]
    before = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in source_files}
    registry = unreal.AssetRegistryHelpers.get_asset_registry()
    registry.scan_paths_synchronous(["/Game/StackVerification"], True)
    dependencies = [str(value) for value in registry.get_dependencies(packages[0], unreal.AssetRegistryDependencyOptions())]
    if packages[3] not in dependencies:
        raise RuntimeError("Mesh material-instance dependency was not discovered")
    options = unreal.MigrationOptions(prompt=False, ignore_dependencies=False, asset_conflict=unreal.AssetMigrationConflict.SKIP)
    tools = unreal.AssetToolsHelpers.get_asset_tools()
    tools.migrate_packages(packages[:2], str(destination), options)
    targets = [destination / "StackVerification" / (name + ".uasset") for name in names]
    if not all(path.is_file() for path in targets):
        raise RuntimeError("Migration did not copy all four native packages")
    first = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in targets}
    tools.migrate_packages(packages[:2], str(destination), options)
    if any(hashlib.sha256(path.read_bytes()).hexdigest() != first[str(path)] for path in targets):
        raise RuntimeError("Existing destination was overwritten")
    if any(hashlib.sha256(path.read_bytes()).hexdigest() != before[str(path)] for path in source_files):
        raise RuntimeError("Source package was changed")
    (project / "Saved" / "stack-migration-probe.json").write_text(json.dumps({
        "sourceUnchanged": True, "skipConflict": True, "dependencies": dependencies,
        "files": first,
    }, indent=2), encoding="utf-8")
    unreal.log("STACK_NATIVE_MIGRATION_OK: dependencies copied, conflicts skipped, source unchanged")
elif project.name == "ue-native-target":
    assets = [unreal.load_asset(package) for package in packages]
    if any(asset is None for asset in assets):
        raise RuntimeError("Migrated native package could not be loaded in the target project")
    classes = [asset.get_class().get_name() for asset in assets]
    if classes != ["StaticMesh", "Texture2D", "Material", "MaterialInstanceConstant"]:
        raise RuntimeError("Migrated native types differ: " + repr(classes))
    unreal.log("STACK_NATIVE_TARGET_LOAD_OK: all four migrated native packages loaded")
else:
    raise RuntimeError("This verification script must never run against a user project")
