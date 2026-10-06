// Adapted from Yashashin/Kura at f8f6d06bf2a626f94cb10b0104b71ce80c6c5458 (MIT).
// Original copyright (c) 2026 Yukio; see KURA-LICENSE.txt.
#pragma once

#include "CoreMinimal.h"
#include "Kismet/BlueprintFunctionLibrary.h"
#include "StackThumbnailLibrary.generated.h"

/** Thumbnail pipeline entry points (SPEC 7.2), called from kura/ingest.py
 *  via the Python-exposed UFUNCTION bridge.
 *
 *  Project assets are never modified: the material preview mesh is applied
 *  transiently for the library PNG render and restored afterwards, so the
 *  project's own Content Browser thumbnails keep the default sphere
 *  (decision 2026-07-12). */
UCLASS()
class UStackThumbnailLibrary : public UBlueprintFunctionLibrary
{
	GENERATED_BODY()

public:
	UFUNCTION(BlueprintCallable, Category = "Stack Asset Library")
	static TArray<FString> GetOpenNativeAssetPaths();

	/** Register a content root in the disposable helper project. Never called by the collection menu. */
	UFUNCTION(BlueprintCallable, Category = "Stack Asset Library")
	static bool MountGameContent(const FString& ContentDirectory);

	/** Render the asset's thumbnail and write it as a PNG (Meta mirror).
	 *  Waits for pending asset/shader compilation first - rendering earlier
	 *  produces default-grid/blank thumbnails. Creates the target folder.
	 *  Resolution comes from config thumb_resolution (SPEC F5). */
	UFUNCTION(BlueprintCallable, Category = "Stack Asset Library")
	static bool ExportThumbnailPng(const FString& PackageName, const FString& OutPngPath,
		int32 Resolution = 256);

	/** Material-family variant: temporarily swap in a preview-mesh thumbnail
	 *  info (USceneThumbnailInfoWithPrimitive with the given mesh and orbit
	 *  angles), render the PNG, then restore the original info and dirty
	 *  state. Falls back to a plain render (default sphere) when the mesh
	 *  fails to load (SPEC 7.2). */
	UFUNCTION(BlueprintCallable, Category = "Stack Asset Library")
	static bool ExportMaterialThumbnailPng(const FString& PackageName, const FString& OutPngPath,
		const FString& PreviewMeshPath, float OrbitPitch, float OrbitYaw, float OrbitZoom,
		int32 Resolution = 256);
};
