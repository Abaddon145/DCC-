"""Exercise UE's real asset editors in the disposable verification project."""
import json
from pathlib import Path

import unreal


saved = Path(unreal.Paths.project_saved_dir())
fixture = json.loads((saved / "stack-probe.json").read_text(encoding="utf-8"))
assets = [unreal.load_asset(record["objectPath"]) for record in fixture["assets"]]
if any(asset is None for asset in assets):
    raise RuntimeError("Native fixture could not be loaded")
editors = unreal.get_editor_subsystem(unreal.AssetEditorSubsystem)
opened = editors.open_editor_for_assets(assets)
edited = set(unreal.StackThumbnailLibrary.get_open_native_asset_paths())
expected = {record["objectPath"] for record in fixture["assets"]}
result = {"opened": opened, "edited": sorted(edited), "expected": sorted(expected)}
(saved / "stack-preview-probe.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
if not opened or not expected.issubset(edited):
    raise RuntimeError("Not all four native asset editors opened")
for asset in assets:
    editors.close_all_editors_for_asset(asset)
unreal.log("STACK_NATIVE_EDITORS_OK: all four native asset editors registered")
