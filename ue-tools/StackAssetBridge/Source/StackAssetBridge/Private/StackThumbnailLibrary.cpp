// Adapted from Yashashin/Kura at f8f6d06bf2a626f94cb10b0104b71ce80c6c5458 (MIT).
// Original copyright (c) 2026 Yukio; see KURA-LICENSE.txt.
#include "StackThumbnailLibrary.h"

#include "AssetCompilingManager.h"
#include "Editor.h"
#include "Components/ReflectionCaptureComponent.h"
#include "Components/SkyLightComponent.h"
#include "Engine/StaticMesh.h"
#include "Engine/StreamableRenderAsset.h"
#include "Engine/Texture.h"
#include "HAL/FileManager.h"
#include "ImageUtils.h"
#include "Materials/MaterialInterface.h"
#include "Misc/EngineVersionComparison.h"
#include "Misc/PackageName.h"
#include "Misc/Paths.h"
#include "ObjectTools.h"
#include "ShaderCompiler.h"
#include "Subsystems/AssetEditorSubsystem.h"
#include "ThumbnailRendering/SceneThumbnailInfoWithPrimitive.h"
#include "ThumbnailRendering/ThumbnailManager.h"
#include "UObject/ObjectKey.h"
#include "UObject/Package.h"
#include "UObject/UObjectIterator.h"
#include "UnrealEdGlobals.h"

DEFINE_LOG_CATEGORY_STATIC(LogKuraThumb, Log, All);

namespace
{
	UObject* LoadPrimaryAsset(const FString& PackageName)
	{
		const FString ObjectPath = PackageName.Contains(TEXT("."))
			? PackageName : PackageName + TEXT(".") + FPackageName::GetShortName(PackageName);
		if (!FPackageName::IsValidObjectPath(ObjectPath))
		{
			return nullptr;
		}
		return LoadObject<UObject>(nullptr, *ObjectPath);
	}

	// Long enough to cover the render; the streamer releases the mips again
	// once it expires, so nothing stays pinned after an ingest batch.
	constexpr float ForceResidentSeconds = 30.f;

	void AddMaterialTextures(UMaterialInterface* Material, TArray<UStreamableRenderAsset*>& Out)
	{
		if (!Material)
		{
			return;
		}
		TArray<UTexture*> Textures;
#if UE_VERSION_OLDER_THAN(5, 7, 0)
		// 5.7 added the defaulted-argument overload; before that the five-argument
		// form is the only one, and in 5.7+ it is deprecated to an empty final {}.
		Material->GetUsedTextures(Textures, EMaterialQualityLevel::Num, true, ERHIFeatureLevel::Num, true);
#else
		Material->GetUsedTextures(Textures);
#endif
		for (UTexture* Texture : Textures)
		{
			if (Texture)
			{
				Out.AddUnique(Texture);
			}
		}
	}

	/** Everything this render samples that streams: the textures behind the
	 *  material(s), plus the mesh itself for LOD streaming. */
	void CollectStreamables(UObject* Asset, TArray<UStreamableRenderAsset*>& Out)
	{
		if (UTexture* Texture = Cast<UTexture>(Asset))
		{
			Out.AddUnique(Texture);
		}
		else if (UMaterialInterface* Material = Cast<UMaterialInterface>(Asset))
		{
			AddMaterialTextures(Material, Out);
		}
		else if (UStaticMesh* Mesh = Cast<UStaticMesh>(Asset))
		{
			Out.AddUnique(Mesh);
			for (const FStaticMaterial& Slot : Mesh->GetStaticMaterials())
			{
				AddMaterialTextures(Slot.MaterialInterface, Out);
			}
		}
	}

	bool AllFullyStreamedIn(const TArray<UStreamableRenderAsset*>& Streamables)
	{
		for (UStreamableRenderAsset* Res : Streamables)
		{
			if (!Res->IsFullyStreamedIn())
			{
				return false;
			}
		}
		return true;
	}

	/** UE builds each thumbnail preview scene lazily, on the renderer's first
	 *  Draw, and that scene only finishes its lighting - the sky/reflection
	 *  capture done by FThumbnailPreviewScene::Tick ->
	 *  FPreviewScene::UpdateCaptureContents - on the next editor tick. Kura
	 *  renders a whole batch inside one blocking Python call, so when that call
	 *  is also what created the scene, the tick never comes and EVERY thumbnail
	 *  in the batch renders with no ambient fill: the unlit side collapses into
	 *  a dark, dithered blotch (2026-08-15, the entire PolyHaven upload; the
	 *  cold render is bit-for-bit reproducible, and one tick is enough to fix
	 *  it for the rest of the session).
	 *
	 *  A throwaway render creates the scene, then we run the capture update the
	 *  tick would have run. Tiny and NeverFlush because the pixels are
	 *  discarded - only the scene matters - and once per renderer, not once per
	 *  asset, so a 3,000-asset upload pays for this about three times. */
	void WarmThumbnailSceneOnce(UObject* Asset)
	{
		static TSet<FObjectKey> Warmed;
		FThumbnailRenderingInfo* Info = UThumbnailManager::Get().GetRenderingInfo(Asset);
		if (!Info || !Info->Renderer)
		{
			return;
		}
		bool bAlreadyWarmed = false;
		Warmed.Add(FObjectKey(Info->Renderer), &bAlreadyWarmed);
		if (bAlreadyWarmed)
		{
			return;
		}
		FObjectThumbnail Discard;
		ThumbnailTools::RenderThumbnail(Asset, 32, 32,
			ThumbnailTools::EThumbnailTextureFlushMode::NeverFlush,
			/*InTextureRenderTargetResource*/ nullptr, &Discard);
		// Exactly what FThumbnailPreviewScene::Tick would do, applied straight to
		// the preview worlds (FPreviewScene::UpdateCaptureContents). Ticking the
		// editor objects instead is NOT an option: FTickableEditorObject::
		// TickObjects asserts (!bIsTickingObjects) whenever Kura is driven by
		// -ExecutePythonScript, which runs the script from an editor tick.
		for (TObjectIterator<UWorld> It; It; ++It)
		{
			UWorld* PreviewWorld = *It;
			if (PreviewWorld && PreviewWorld->WorldType == EWorldType::EditorPreview)
			{
				USkyLightComponent::UpdateSkyCaptureContents(PreviewWorld);
				UReflectionCaptureComponent::UpdateReflectionCaptureContents(
					PreviewWorld, nullptr, /*bVerifyOnlyCapturing*/ false,
					/*bCapturingForMobile*/ false, /*bInsideTick*/ true);
			}
		}
	}

	bool RenderToPng(UObject* Asset, const FString& OutPngPath, int32 Resolution,
		UStaticMesh* PreviewMesh = nullptr)
	{
		// Commandlets have no full editor thumbnail manager. A PNG can otherwise be all black.
		if (IsRunningCommandlet() || !GUnrealEd)
		{
			UE_LOG(LogKuraThumb, Warning, TEXT("Native thumbnails require the full editor execution entry"));
			return false;
		}
		const uint32 ThumbSize = FMath::Clamp(Resolution, 32, 2048);
		WarmThumbnailSceneOnce(Asset);
		// RenderThumbnail, not GenerateThumbnailForObjectToSaveToDisk: the
		// latter returns null for classes whose thumbnails can always render
		// in real time (textures) - it exists to embed thumbs in packages,
		// not to rasterize them (T_KuraTest had no PNG, 2026-07-12).
		FObjectThumbnail Thumb;
		TArray<UStreamableRenderAsset*> Streamables;
		bool bStreamRetryUsed = false;
		for (int32 Attempt = 0; Attempt < 3; ++Attempt)
		{
			// UE5 compiles textures/meshes/shaders asynchronously; rendering
			// before they finish captures the default-grid fallback. The
			// render itself can enqueue new shader jobs (first render of a
			// material instance), so loop until a render saw no compiling.
			FAssetCompilingManager::Get().FinishAllCompilation();
			if (GShaderCompilingManager)
			{
				GShaderCompilingManager->FinishAllCompilation();
			}

			// Compilation is not residency: a fully BUILT texture whose high
			// mips have not streamed in still renders blurry - the same reason
			// the Content Browser only sharpens an asset after you click it
			// (clicking bumps its streaming priority). Force full mips/LODs and
			// block until the streamer delivers them. Collected after the
			// compilation waits so a just-compiled material reports its
			// textures.
			Streamables.Reset();
			CollectStreamables(Asset, Streamables);
			if (PreviewMesh)
			{
				Streamables.AddUnique(PreviewMesh);
			}
			for (UStreamableRenderAsset* Res : Streamables)
			{
				Res->SetForceMipLevelsToBeResident(ForceResidentSeconds);
				Res->WaitForStreaming();
			}

			Thumb = FObjectThumbnail();
			ThumbnailTools::RenderThumbnail(Asset, ThumbSize, ThumbSize,
				ThumbnailTools::EThumbnailTextureFlushMode::AlwaysFlush,
				/*InTextureRenderTargetResource*/ nullptr, &Thumb);

			const bool bShadersPending = GShaderCompilingManager && GShaderCompilingManager->IsCompiling();
			// One streaming retry only: WaitForStreaming already blocked, so a
			// second wait rarely helps, and assets whose optional mips are not
			// mounted never report fully streamed (would cost every render the
			// full retry budget).
			const bool bStreamPending = !bStreamRetryUsed && !AllFullyStreamedIn(Streamables);
			bStreamRetryUsed = bStreamRetryUsed || bStreamPending;
			if (!bShadersPending && !bStreamPending)
			{
				break;
			}
		}
		if (!AllFullyStreamedIn(Streamables))
		{
			UE_LOG(LogKuraThumb, Log,
				TEXT("%s rendered before streaming reported full residency; the thumbnail may be soft"),
				*Asset->GetPathName());
		}

		const TArray<uint8>& Bgra = Thumb.GetUncompressedImageData();
		if (Bgra.Num() == 0 || Thumb.GetImageWidth() <= 0 || Thumb.GetImageHeight() <= 0)
		{
			UE_LOG(LogKuraThumb, Warning, TEXT("Empty thumbnail for %s"), *Asset->GetPathName());
			return false;
		}

		// Force opaque: texture thumbnails carry the source alpha straight
		// into the render target (T_KuraTest came out A=0 everywhere) and a
		// fully transparent PNG reads as "no thumbnail" in the panel.
		TArray<uint8> Opaque(Bgra);
		for (int32 Idx = 3; Idx < Opaque.Num(); Idx += 4)
		{
			Opaque[Idx] = 255;
		}

		IFileManager::Get().MakeDirectory(*FPaths::GetPath(OutPngPath), /*Tree*/ true);
		const FImageView Image(Opaque.GetData(),
			Thumb.GetImageWidth(), Thumb.GetImageHeight(), ERawImageFormat::BGRA8);
		if (!FImageUtils::SaveImageByExtension(*OutPngPath, Image))
		{
			UE_LOG(LogKuraThumb, Warning, TEXT("Failed to write %s"), *OutPngPath);
			return false;
		}
		return true;
	}
}

TArray<FString> UStackThumbnailLibrary::GetOpenNativeAssetPaths()
{
	TArray<FString> Paths;
	if (GEditor)
	{
		for (UObject* Asset : GEditor->GetEditorSubsystem<UAssetEditorSubsystem>()->GetAllEditedAssets())
		{
			if (Asset) Paths.Add(Asset->GetPathName());
		}
	}
	return Paths;
}

bool UStackThumbnailLibrary::MountGameContent(const FString& ContentDirectory)
{
	if (FPaths::IsRelative(ContentDirectory) || !IFileManager::Get().DirectoryExists(*ContentDirectory))
	{
		return false;
	}
	FString ContentPath = FPaths::ConvertRelativePathToFull(ContentDirectory);
	FPaths::NormalizeDirectoryName(ContentPath);
	FPackageName::RegisterMountPoint(TEXT("/Game/"), ContentPath + TEXT("/"));
	return true;
}

bool UStackThumbnailLibrary::ExportThumbnailPng(const FString& PackageName, const FString& OutPngPath,
	int32 Resolution)
{
	UObject* Asset = LoadPrimaryAsset(PackageName);
	if (!Asset)
	{
		UE_LOG(LogKuraThumb, Warning, TEXT("Cannot load %s for thumbnail export"), *PackageName);
		return false;
	}
	return RenderToPng(Asset, OutPngPath, Resolution);
}

bool UStackThumbnailLibrary::ExportMaterialThumbnailPng(const FString& PackageName, const FString& OutPngPath,
	const FString& PreviewMeshPath, float OrbitPitch, float OrbitYaw, float OrbitZoom,
	int32 Resolution)
{
	UObject* Asset = LoadPrimaryAsset(PackageName);
	UMaterialInterface* Material = Cast<UMaterialInterface>(Asset);
	if (!Material)
	{
		UE_LOG(LogKuraThumb, Warning, TEXT("Cannot load material %s for thumbnail export"), *PackageName);
		return false;
	}

	UStaticMesh* Mesh = Cast<UStaticMesh>(LoadPrimaryAsset(PreviewMeshPath));
	if (!Mesh)
	{
		UE_LOG(LogKuraThumb, Warning,
			TEXT("Preview mesh %s failed to load; rendering %s with the default sphere (SPEC 7.2)"),
			*PreviewMeshPath, *PackageName);
		return RenderToPng(Material, OutPngPath, Resolution);
	}

	// Swap in a transient thumbnail info for this render only: the project
	// asset must keep its own (default) thumbnail - never dirty, never save.
	UPackage* Package = Material->GetOutermost();
	const bool bWasDirty = Package->IsDirty();
	UThumbnailInfo* OriginalInfo = Material->ThumbnailInfo;

	USceneThumbnailInfoWithPrimitive* TempInfo =
		NewObject<USceneThumbnailInfoWithPrimitive>(Material, NAME_None, RF_Transient);
	TempInfo->PreviewMesh = FSoftObjectPath(Mesh);
	TempInfo->PrimitiveType = TPT_None; // use the custom mesh
	TempInfo->OrbitPitch = OrbitPitch;
	TempInfo->OrbitYaw = OrbitYaw;
	TempInfo->OrbitZoom = OrbitZoom;
	Material->ThumbnailInfo = TempInfo;

	const bool bOk = RenderToPng(Material, OutPngPath, Resolution, Mesh);

	Material->ThumbnailInfo = OriginalInfo;
	if (!bWasDirty)
	{
		Package->SetDirtyFlag(false);
	}
	return bOk;
}
